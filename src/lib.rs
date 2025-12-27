//! A fixed-capacity [`Vec`] which allows concurrences reads and
//! spin-lock writes.
//!
//! [`AtomicVec`] is designed for situations where reads need to
//! be extremely fast and cannot be blocked by writes. The
//! capacity is fixed and defined on creation, and cannot be
//! greater than [`isize::MAX`].
#![feature(allocator_api, sized_type_properties)]

mod cap;
pub mod error;
pub mod guard;
mod raw;
#[cfg(test)]
mod tests;

use {
    crate::{
        cap::Cap, error::TryReserveError, guard::AtomicVecGuard,
        raw::RawAtomicVec,
    },
    std::{
        alloc::{Allocator, Global},
        ops,
        ptr::NonNull,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    },
};

/// A fixed-capacity [`Vec`] which allows concurrences reads and
/// spin-lock writes.
pub struct AtomicVec<T, A: Allocator = Global> {
    buf: RawAtomicVec<T, A>,
    len: AtomicUsize,
    mutex: Mutex<()>,
}
/// SAFETY: `AtomicVec` owns its data (the `RawAtomicVec` buffer).
/// It is safe to send it to another thread if the elements `T`
/// can be sent and the allocator `A` is also `Send`.
/// Since we have exclusive ownership of the buffer, no other thread
/// can access the data while it is being moved.
unsafe impl<T: Send, A: Allocator + Send> Send for AtomicVec<T, A> {}
/// SAFETY: `AtomicVec` is safe to share between threads because:
/// 1. Concurrent writes are synchronized via a `Mutex`.
/// 2. Concurrent reads (via `as_slice` or `index`) are synchronized with writes
///    using `Ordering::Acquire` and `Ordering::Release` on the `len` field,
///    ensuring memory visibility.
/// 3. The heap pointer in `RawAtomicVec` is immutable for the lifetime of the
///    vector, preventing Use-After-Free during concurrent access. This is valid
///    only if `T` is `Sync`.
unsafe impl<T: Sync, A: Allocator + Sync> Sync for AtomicVec<T, A> {}

/// Getters
impl<T, A: Allocator> AtomicVec<T, A> {
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        self.buf.ptr()
    }
    #[inline]
    #[must_use]
    pub const fn as_mut_ptr(&self) -> *mut T {
        self.buf.ptr()
    }
    #[inline]
    #[must_use]
    pub const fn as_non_null(&self) -> NonNull<T> {
        self.buf.non_null()
    }
}

impl<T, A: Allocator> AtomicVec<T, A> {
    /// Constructs a new [`AtomicVec<T>`] in the provided allocator,
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
    /// use atomicvec::AtomicVec;
    /// use std::alloc::System;
    ///
    /// let my_atomic_vec = AtomicVec::try_new_in(10, System);
    /// ```
    pub fn try_new_in(
        capacity: usize,
        alloc: A,
    ) -> Result<Self, TryReserveError> {
        let Some(cap) = Cap::try_new::<T>(capacity) else {
            return Err(TryReserveError::CapacityOverflow);
        };
        let buf = RawAtomicVec::try_new_in(cap, alloc)?;

        Ok(Self {
            buf,
            len: AtomicUsize::new(0),
            mutex: Mutex::new(()),
        })
    }

    /// Constructs a new [`AtomicVec<T>`] in the provided allocator.
    ///
    /// # Examples
    /// ```
    /// #![feature(allocator_api)]
    /// use atomicvec::AtomicVec;
    /// use std::alloc::System;
    ///
    /// let my_atomic_vec = AtomicVec::new_in(10, System);
    /// ```
    #[inline]
    #[must_use]
    #[allow(clippy::missing_panics_doc)]
    pub fn new_in(capacity: usize, alloc: A) -> Self {
        let cap = Cap::try_new::<T>(capacity)
            .unwrap_or_else(|| panic!("{}", TryReserveError::CapacityOverflow));
        let buf = RawAtomicVec::new_in(cap, alloc);

        Self {
            buf,
            len: AtomicUsize::new(0),
            mutex: Mutex::new(()),
        }
    }

    #[inline]
    pub fn lock(&self) -> Option<AtomicVecGuard<'_, T, A>> {
        let guard = self.mutex.lock().ok()?;

        Some(AtomicVecGuard {
            _guard: guard,
            vec: self,
        })
    }
}

impl<T> AtomicVec<T> {
    /// Constructs a new [`AtomicVec<T>`],
    /// returning an error if the allocation fails
    ///
    /// # Errors
    /// Returns an error if:
    /// * `cap * size_of::<T>` overflows `isize::MAX`
    /// * memory is exhausted
    ///
    /// # Examples
    /// ```
    /// use atomicvec::AtomicVec;
    ///
    /// let my_atomic_vec = AtomicVec::try_new(10);
    /// ```
    #[inline]
    pub fn try_new(capacity: usize) -> Result<Self, TryReserveError> {
        Self::try_new_in(capacity, Global)
    }

    /// Constructs a new [`RawAtomicVec<T>`].
    ///
    /// # Examples
    /// ```
    /// use atomicvec::AtomicVec;
    ///
    /// let my_atomic_vec = AtomicVec::new(10);
    /// ```
    #[inline]
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self::new_in(capacity, Global)
    }
}
impl<T, A: Allocator> ops::Index<usize> for AtomicVec<T, A> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.len());
        // SAFETY:
        // `index` is inside the allocated block and
        // the data at that index is already initialized.
        // `index < capacity` so this cannot overflow isize.
        unsafe { self.as_non_null().add(index).as_ref() }
    }
}
