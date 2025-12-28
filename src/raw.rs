use {
    crate::{cap::Cap, error::TryReserveError},
    std::{
        alloc::{Allocator, Global, Layout, handle_alloc_error},
        marker::PhantomData,
        mem::SizedTypeProperties as _,
        ptr::NonNull,
    },
};

/// Read-only data of the [`AtomicVec`](crate::AtomicVec).
///
/// You can push data into this only through [`AtomicVec`](crate::AtomicVec).
pub(crate) struct RawAtomicVec<T, A: Allocator = Global> {
    /// Pointer to the first byte of the buffer.
    ///
    /// Changes to this field are `Undefined Behavior`
    ptr: NonNull<u8>,
    /// Capacity of the buffer.
    ///
    /// Cannot exceed [`isize::MAX`]
    cap: Cap,
    alloc: A,
    _marker: PhantomData<T>,
}

impl<T, A: Allocator> RawAtomicVec<T, A> {
    /// Constructs a new [`RawAtomicVec<T>`] in the provided allocator,
    /// returning an error if the allocation fails
    ///
    /// # Errors
    /// Returns an error if:
    /// * `cap * size_of::<T>` overflows `isize::MAX`
    /// * memory is exhausted
    pub(crate) fn try_new_in(
        cap: Cap,
        alloc: A,
    ) -> Result<Self, TryReserveError> {
        // `cap` for ZST is zero.
        if cap == Cap::ZERO {
            return Ok(Self {
                ptr: NonNull::dangling(),
                cap,
                alloc,
                _marker: PhantomData,
            });
        }

        let Ok(layout) = Layout::array::<T>(cap.get()) else {
            return Err(TryReserveError::CapacityOverflow);
        };

        let Ok(block) = alloc.allocate(layout) else {
            return Err(TryReserveError::AllocError(layout));
        };

        let cap = block.len() / size_of::<T>();
        // SAFETY: `cap` is derived from a valid allocation,
        //          so it can't exceed an isize.
        let cap = unsafe { Cap::new_unchecked::<T>(cap) };
        let ptr = block.cast::<u8>();

        Ok(Self {
            ptr,
            cap,
            alloc,
            _marker: PhantomData,
        })
    }
    /// Constructs a new [`RawAtomicVec<T>`] in the provided allocator.
    #[inline]
    pub(crate) fn new_in(cap: Cap, alloc: A) -> Self {
        match Self::try_new_in(cap, alloc) {
            Ok(this) => this,
            Err(e @ TryReserveError::CapacityOverflow) => panic!("{e}"),
            Err(TryReserveError::AllocError(layout)) => {
                handle_alloc_error(layout)
            }
        }
    }
    /// Constructs a new [`RawAtomicVec<T>`] directly from a
    /// [`NonNull`] pointer, a capacity, and an allocator.
    ///
    /// # Safety
    /// * `ptr` must be currently allocated with the given allocator `alloc`.
    /// * `T` needs to have the same alignment as what `ptr` was allocated
    ///   with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * capacity needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`]
    #[inline]
    #[must_use]
    pub(crate) unsafe fn from_nonnull_in(ptr: NonNull<T>, cap: Cap, alloc: A) -> Self {
        Self {
            ptr: ptr.cast(),
            cap,
            alloc,
            _marker: PhantomData,
        }
    }
    /// Constructs a new [`RawAtomicVec<T>`] directly from a pointer,
    /// a capacity, and an allocator.
    ///
    /// # Safety
    /// * `ptr` must be currently allocated with the given allocator `alloc`.
    /// * `T` needs to have the same alignment as what `ptr` was allocated
    ///   with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * capacity needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`]
    #[inline]
    #[must_use]
    pub(crate) unsafe fn from_raw_in(ptr: *mut T, cap: Cap, alloc: A) -> Self {
        Self {
            // SAFETY: the safety contract must be upheld by the caller.
            ptr: unsafe { NonNull::new_unchecked(ptr).cast() },
            cap,
            alloc,
            _marker: PhantomData,
        }
    }
    #[inline]
    pub(crate) const fn non_null(&self) -> NonNull<T> {
        self.ptr.cast()
    }
    #[inline]
    pub(crate) const fn ptr(&self) -> *mut T {
        self.non_null().as_ptr()
    }
    #[inline]
    pub(crate) const fn capacity(&self) -> usize {
        if T::IS_ZST {
            usize::MAX
        } else {
            self.cap.get()
        }
    }
    #[inline]
    pub(crate) const fn allocator(&self) -> &A {
        &self.alloc
    }
}

impl<T, A: Allocator> Drop for RawAtomicVec<T, A> {
    fn drop(&mut self) {
        // if T::IS_ZST then cap is zero
        if self.cap == Cap::ZERO {
            return;
        }

        let size = size_of::<T>() * self.cap.get();
        let align = align_of::<T>();
        // SAFETY:
        //        1. `!T::IS_ZST`
        //        2. `size <= isize::MAX` is already checked in the constructor
        //        3. the constructed layout is the same as in the constructor
        unsafe {
            let layout = Layout::from_size_align_unchecked(size, align);
            self.alloc.deallocate(self.ptr, layout);
        }
    }
}
