#![doc = include_str!("../docs/lib-docs.md")]
//! # Examples
//! ```
#![doc = include_str!("../examples/basic_usage.rs")]
//! ```
#![feature(allocator_api, sized_type_properties)]

mod cap;
pub mod error;
pub mod guard;
mod macros;
mod raw;
#[cfg(test)]
mod tests;

use {
    crate::{
        cap::Cap, error::TryReserveError, guard::GrowGuard, raw::RawGrowLock,
    },
    std::{
        alloc::{Allocator, Global},
        borrow::Borrow,
        fmt,
        hash::{Hash, Hasher},
        mem::ManuallyDrop,
        ops,
        ptr::{self, NonNull},
        slice::{self, SliceIndex},
        sync::{
            LockResult, Mutex, PoisonError, TryLockError, TryLockResult,
            atomic::{AtomicUsize, Ordering},
        },
    },
};

#[doc = include_str!("../docs/growlock.md")]
/// # Examples
/// ```
#[doc = include_str!("../examples/basic_usage.rs")]
/// ```
pub struct GrowLock<T, A: Allocator = Global> {
    buf: RawGrowLock<T, A>,
    len: AtomicUsize,
    mutex: Mutex<()>,
}

/// # Safety:
/// If both `T` and `A` are [`Send`], it is safe to transfer an
/// [`GrowLock<T, A>`] between threads as we have exclusive ownership of the
/// buffer.
///
/// No thread can access the data while it's being moved.
unsafe impl<T, A> Send for GrowLock<T, A>
where
    T: Send,
    A: Send + Allocator,
{
}
/// # Safety:
/// If both `T` and `A` are [`Sync`], there's no interior mutability outside
/// the [`mutex`](Mutex) and the [`len`](AtomicUsize) (which is thread-safe).
///
/// All writes to the buffer are handled along the [`mutex`](Mutex), and so
/// this collection is [`Sync`]
unsafe impl<T, A> Sync for GrowLock<T, A>
where
    T: Sync + Send,
    A: Sync + Allocator,
{
}

impl<T, A: Allocator> GrowLock<T, A> {
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }
    #[inline]
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.buf.capacity()
    }
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }
    #[inline]
    #[must_use]
    pub const fn allocator(&self) -> &A {
        self.buf.allocator()
    }
    #[inline]
    #[must_use]
    pub const fn as_ptr(&self) -> *const T {
        self.buf.as_ptr()
    }
    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.as_mut_ptr()
    }
    #[inline]
    #[must_use]
    pub const fn as_non_null(&mut self) -> NonNull<T> {
        self.buf.as_non_null()
    }
    /// SAFETY:
    /// calling this method is safe, but using the ptr is not. It's okay
    /// because this is private and only used in the guard.
    #[inline]
    #[must_use]
    pub(crate) const unsafe fn as_non_null_ref(&self) -> NonNull<T> {
        self.buf.as_non_null()
    }
    /// UB: if the slice is empty
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        // SAFETY:
        // * `self.as_ptr()` is never null, and valid for reads up to
        //   `self.len()` if we can have a reference to `self` (which we do)
        // * the entire block of memory is within a single allocation
        // * at least `self.len()` number of elements are correctly initialized.
        // * `capacity * size_of::<T>()` doesn't overflow `isize::MAX`, so
        //   neither does `self.len() * size_of::<T>()`
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len()) }
    }

    /// Constructs a new [`GrowLock<T>`] in the provided allocator,
    /// returning an error if the allocation fails
    ///
    /// # Errors
    /// Returns an error if:
    /// * `cap * size_of::<T>` overflows [`isize::MAX`]
    /// * memory is exhausted
    ///
    /// # Examples
    /// ```
    /// #![feature(allocator_api)]
    /// use growlock::GrowLock;
    /// use std::alloc::System;
    ///
    /// let my_atomic_vec: GrowLock<u32, _> = GrowLock::try_with_capacity_in(10, System).unwrap();
    /// ```
    pub fn try_with_capacity_in(
        capacity: usize,
        alloc: A,
    ) -> Result<Self, TryReserveError> {
        let Some(cap) = Cap::new::<T>(capacity) else {
            return Err(TryReserveError::CapacityOverflow);
        };
        let buf = RawGrowLock::try_with_capacity_in(cap, alloc)?;

        Ok(Self {
            buf,
            len: AtomicUsize::new(0),
            mutex: Mutex::new(()),
        })
    }

    /// Constructs a new [`GrowLock<T>`] in the provided allocator.
    ///
    /// # Examples
    /// ```
    /// #![feature(allocator_api)]
    /// use growlock::GrowLock;
    /// use std::alloc::System;
    ///
    /// let my_atomic_vec: GrowLock<u32, _> = GrowLock::with_capacity_in(10, System);
    /// ```
    #[inline]
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn with_capacity_in(capacity: usize, alloc: A) -> Self {
        let cap = Cap::new::<T>(capacity)
            .unwrap_or_else(|| panic!("{}", TryReserveError::CapacityOverflow));
        let buf = RawGrowLock::with_capacity_in(cap, alloc);

        Self {
            buf,
            len: AtomicUsize::new(0),
            mutex: Mutex::new(()),
        }
    }
    /// Constructs a new [`GrowLock<T>`] directly from a [`NonNull`] pointer,
    /// a capacity, and an allocator.
    ///
    /// # Safety
    /// * `ptr` must be currently allocated with the given allocator `alloc`.
    /// * `T` needs to have the same alignment as what `ptr` was allocated with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * `capacity` needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`] (the size is
    ///   `self.capacity() * size_of::<T>`)
    /// * `len` must be <= `capacity`
    /// * at least `len` elements starting from `ptr` need to be properly
    ///   initialized values of type `T`.
    #[inline]
    pub unsafe fn from_parts_in(
        ptr: NonNull<T>,
        len: usize,
        capacity: usize,
        alloc: A,
    ) -> Self {
        Self {
            // SAFETY: the safety contract must be upheld by the caller
            buf: unsafe {
                RawGrowLock::from_nonnull_in(
                    ptr,
                    Cap::new_unchecked::<T>(capacity),
                    alloc,
                )
            },
            len: AtomicUsize::new(len),
            mutex: Mutex::new(()),
        }
    }
    /// Constructs a new [`GrowLock<T>`] directly from a pointer,
    /// a capacity, and an allocator.
    ///
    /// # Safety
    /// * `ptr` must be currently allocated with the given allocator `alloc`.
    /// * `T` needs to have the same alignment as what `ptr` was allocated with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * `capacity` needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`]
    /// * `len` must be <= `capacity`
    /// * at least `len` elements starting from `ptr` need to be properly
    ///   initialized values of type `T`.
    #[inline]
    pub unsafe fn from_raw_parts_in(
        ptr: *mut T,
        len: AtomicUsize,
        capacity: usize,
        alloc: A,
    ) -> Self {
        Self {
            // SAFETY: the  safety contract must be upheld by the caller
            buf: unsafe {
                RawGrowLock::from_raw_in(
                    ptr,
                    Cap::new_unchecked::<T>(capacity),
                    alloc,
                )
            },
            len,
            mutex: Mutex::new(()),
        }
    }

    #[inline]
    pub fn write(&self) -> LockResult<GrowGuard<'_, T, A>> {
        match self.mutex.lock() {
            Ok(guard) => Ok(GrowGuard::new(self, guard)),
            Err(e) => {
                let guard = e.into_inner();
                Err(PoisonError::new(GrowGuard::new(self, guard)))
            }
        }
    }
    #[inline]
    pub fn try_write(&self) -> TryLockResult<GrowGuard<'_, T, A>> {
        match self.mutex.try_lock() {
            Ok(guard) => Ok(GrowGuard::new(self, guard)),
            Err(TryLockError::Poisoned(e)) => {
                let guard = e.into_inner();
                Err(TryLockError::Poisoned(PoisonError::new(GrowGuard::new(
                    self, guard,
                ))))
            }

            Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
        }
    }
    /// Decomposes a [`GrowLock<T>`] into its raw components:
    /// ([`NonNull`] pointer, length, capacity, allocator).
    ///
    /// After calling this function, the caller is responsible for cleaning up
    /// the [`GrowLock<T>`]. Most often, you can do this by calling
    /// [`from_parts_in`](GrowLock::from_parts_in).
    pub fn into_parts_with_alloc(self) -> (NonNull<T>, usize, usize, A) {
        let mut this = ManuallyDrop::new(self);
        let ptr = this.as_non_null();
        let len = this.len();
        let cap = this.capacity();
        // SAFETY: `this.allocator()` is a reference
        // so all precondition are satisfied.
        let alloc = unsafe { ptr::read(this.allocator()) };
        (ptr, len, cap, alloc)
    }
    /// Decomposes a [`GrowLock<T>`] into its raw components:
    /// (pointer, length, capacity, allocator).
    ///
    /// After calling this function, the caller is responsible for cleaning up
    /// the [`GrowLock<T>`]. Most often, you can do this by calling
    /// [`from_raw_parts_in`](GrowLock::from_raw_parts_in).
    #[inline]
    pub fn into_raw_parts_with_alloc(self) -> (*mut T, usize, usize, A) {
        let (ptr, len, cap, alloc) = self.into_parts_with_alloc();
        let ptr = ptr.as_ptr();
        (ptr, len, cap, alloc)
    }
}

impl<T> GrowLock<T> {
    /// Constructs a new [`GrowLock<T>`],
    /// returning an error if the allocation fails
    ///
    /// # Errors
    /// Returns an error if:
    /// * `cap * size_of::<T>` overflows `isize::MAX`
    /// * memory is exhausted
    ///
    /// # Examples
    /// ```
    /// use growlock::GrowLock;
    ///
    /// let my_atomic_vec: GrowLock<()> = GrowLock::try_with_capacity(10).unwrap();
    /// ```
    #[inline]
    pub fn try_with_capacity(capacity: usize) -> Result<Self, TryReserveError> {
        Self::try_with_capacity_in(capacity, Global)
    }

    /// Constructs a new [`GrowLock<T>`].
    ///
    /// # Examples
    /// ```
    /// use growlock::GrowLock;
    ///
    /// let my_atomic_vec: GrowLock<String> = GrowLock::with_capacity(10);
    /// ```
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacity_in(capacity, Global)
    }

    /// Constructs a new [`GrowLock<T>`] directly from a [`NonNull`] pointer,
    /// and a capacity.
    ///
    /// # Safety
    /// * `ptr` must be currently allocated with the global allocator.
    /// * `T` needs to have the same alignment as what `ptr` was allocated with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * `capacity` needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`]
    /// * `len` must be <= `capacity`
    /// * at least `len` elements starting from `ptr` need to be properly
    ///   initialized values of type `T`.
    #[inline]
    pub unsafe fn from_parts(
        ptr: NonNull<T>,
        len: AtomicUsize,
        capacity: usize,
    ) -> Self {
        Self {
            // SAFETY: the  safety contract must be upheld by the caller
            buf: unsafe {
                RawGrowLock::from_nonnull_in(
                    ptr,
                    Cap::new_unchecked::<T>(capacity),
                    Global,
                )
            },
            len,
            mutex: Mutex::new(()),
        }
    }
    /// Constructs a new [`GrowLock<T>`] directly from a pointer, and
    /// a capacity.
    ///
    /// # Safety
    /// * `ptr` must be currently allocated with the global allocator.
    /// * `T` needs to have the same alignment as what `ptr` was allocated with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * `capacity` needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`]
    /// * `len` must be <= `capacity`
    /// * at least `len` elements starting from `ptr` need to be properly
    ///   initialized values of type `T`.
    #[inline]
    pub unsafe fn from_raw_parts(
        ptr: *mut T,
        len: AtomicUsize,
        capacity: usize,
    ) -> Self {
        Self {
            // SAFETY: the  safety contract must be upheld by the caller
            buf: unsafe {
                RawGrowLock::from_raw_in(
                    ptr,
                    Cap::new_unchecked::<T>(capacity),
                    Global,
                )
            },
            len,
            mutex: Mutex::new(()),
        }
    }
    /// Decomposes a [`GrowLock<T>`] into its raw components:
    /// ([`NonNull`] pointer, length, capacity).
    ///
    /// After calling this function, the caller is responsible for cleaning up
    /// the [`GrowLock<T>`]. Most often, you can do this by calling
    /// [`from_parts`](GrowLock::from_parts).
    #[inline]
    pub fn into_parts(self) -> (NonNull<T>, usize, usize) {
        let mut this = ManuallyDrop::new(self);
        (this.as_non_null(), this.len(), this.capacity())
    }
    /// Decomposes a [`GrowLock<T>`] into its raw components:
    /// (pointer, length, capacity).
    ///
    /// After calling this function, the caller is responsible for cleaning up
    /// the [`GrowLock<T>`]. Most often, you can do this by calling
    /// [`from_raw_parts`](GrowLock::from_raw_parts).
    #[inline]
    pub fn into_raw_parts(self) -> (*mut T, usize, usize) {
        let mut this = ManuallyDrop::new(self);
        (this.as_mut_ptr(), this.len(), this.capacity())
    }
}
impl<T, A: Allocator> Drop for GrowLock<T, A> {
    fn drop(&mut self) {
        // if `T::IS_ZST` then `capacity()` returns `usize::MAX`
        if self.capacity() == 0 {
            return;
        }
        // SAFETY: all elements are correctly aligned.
        //  see AtomicVec::as_slice for safety.
        unsafe {
            ptr::drop_in_place(ptr::slice_from_raw_parts_mut(
                self.as_mut_ptr(),
                self.len(),
            ));
        }
    }
}

impl<T, A: Allocator> ops::Deref for GrowLock<T, A> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}
impl<T, A: Allocator> Borrow<[T]> for GrowLock<T, A> {
    #[inline]
    fn borrow(&self) -> &[T] {
        self.as_slice()
    }
}
impl<T, A: Allocator> AsRef<[T]> for GrowLock<T, A> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, I, A> ops::Index<I> for GrowLock<T, A>
where
    I: SliceIndex<[T]>,
    A: Allocator,
{
    type Output = <I as SliceIndex<[T]>>::Output;
    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        ops::Index::index(&**self, index)
    }
}
impl<T, A: Allocator + Default> Default for GrowLock<T, A> {
    #[inline]
    fn default() -> Self {
        Self::with_capacity_in(0, A::default())
    }
}

// ------------------------------- fmt impl -------------------------------

impl<T: fmt::Debug, A: Allocator> fmt::Debug for GrowLock<T, A> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

// ----------------------------- From impl -----------------------------

impl<T, A: Allocator> From<Vec<T, A>> for GrowLock<T, A> {
    #[inline]
    fn from(value: Vec<T, A>) -> Self {
        let (ptr, len, cap, alloc) = value.into_parts_with_alloc();
        // SAFETY: the `AtomicVec` is constructed from parts of the given `Vec`
        // so this is safe.
        unsafe { Self::from_parts_in(ptr, len, cap, alloc) }
    }
}
impl<T, A: Allocator> From<GrowLock<T, A>> for Vec<T, A> {
    #[inline]
    fn from(value: GrowLock<T, A>) -> Self {
        let (ptr, len, cap, alloc) = value.into_parts_with_alloc();
        // SAFETY: the `Vec` is constructed from parts of the given `AtomicVec`
        // so this is safe.
        unsafe { Self::from_parts_in(ptr, len, cap, alloc) }
    }
}

// ----------------------------- PartialEq impl -----------------------------

impl<T, U, A, A2> PartialEq<GrowLock<U, A2>> for GrowLock<T, A>
where
    T: PartialEq<U>,
    A: Allocator,
    A2: Allocator,
{
    #[inline]
    fn eq(&self, rhs: &GrowLock<U, A2>) -> bool {
        PartialEq::eq(&**self, &**rhs)
    }
}
impl<T, U, A> PartialEq<[U]> for GrowLock<T, A>
where
    T: PartialEq<U>,
    A: Allocator,
{
    #[inline]
    fn eq(&self, rhs: &[U]) -> bool {
        PartialEq::eq(&**self, rhs)
    }
}
impl<T, U, A> PartialEq<GrowLock<U, A>> for [T]
where
    T: PartialEq<U>,
    A: Allocator,
{
    fn eq(&self, rhs: &GrowLock<U, A>) -> bool {
        PartialEq::eq(self, &**rhs)
    }
}
impl<T, U, A> PartialEq<&[U]> for GrowLock<T, A>
where
    T: PartialEq<U>,
    A: Allocator,
{
    #[inline]
    fn eq(&self, rhs: &&[U]) -> bool {
        PartialEq::eq(&**self, *rhs)
    }
}
impl<T, U, A> PartialEq<GrowLock<U, A>> for &[T]
where
    T: PartialEq<U>,
    A: Allocator,
{
    fn eq(&self, rhs: &GrowLock<U, A>) -> bool {
        PartialEq::eq(*self, &**rhs)
    }
}
impl<T, U, A> PartialEq<&mut [U]> for GrowLock<T, A>
where
    T: PartialEq<U>,
    A: Allocator,
{
    #[inline]
    fn eq(&self, rhs: &&mut [U]) -> bool {
        PartialEq::eq(&**self, *rhs)
    }
}
impl<T, U, A> PartialEq<GrowLock<U, A>> for &mut [T]
where
    T: PartialEq<U>,
    A: Allocator,
{
    fn eq(&self, rhs: &GrowLock<U, A>) -> bool {
        PartialEq::eq(*self, &**rhs)
    }
}
impl<T, U, A, const N: usize> PartialEq<[U; N]> for GrowLock<T, A>
where
    T: PartialEq<U>,
    A: Allocator,
{
    #[inline]
    fn eq(&self, rhs: &[U; N]) -> bool {
        PartialEq::eq(&**self, rhs)
    }
}
impl<T, U, A, const N: usize> PartialEq<GrowLock<U, A>> for [T; N]
where
    T: PartialEq<U>,
    A: Allocator,
{
    fn eq(&self, rhs: &GrowLock<U, A>) -> bool {
        PartialEq::eq(self, &**rhs)
    }
}
impl<T, U, A, A2> PartialEq<Vec<U, A2>> for GrowLock<T, A>
where
    T: PartialEq<U>,
    A: Allocator,
    A2: Allocator,
{
    fn eq(&self, rhs: &Vec<U, A2>) -> bool {
        PartialEq::eq(&**self, &**rhs)
    }
}

// ----------------------------- Eq and Hash impl -----------------------------

impl<T: Eq, A: Allocator> Eq for GrowLock<T, A> {}
/// [`GrowLock`] implements [`Borrow<[T]>`], so we need to `hash` the
/// same way as the slice does.
impl<T: Hash, A: Allocator> Hash for GrowLock<T, A> {
    /// [`GrowLock`] implements [`Borrow<[T]>`], so we need to `hash` the
    /// same way as the slice does.
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        Hash::hash(&**self, state);
    }
}
