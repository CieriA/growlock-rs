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
    pub(crate) fn try_with_capacity_in(
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
    pub(crate) fn with_capacity_in(cap: Cap, alloc: A) -> Self {
        match Self::try_with_capacity_in(cap, alloc) {
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
    /// * `T` needs to have the same alignment as what `ptr` was allocated with.
    /// * `size_of::<T>() * cap` must be the same as the size the pointer was
    ///   allocated with.
    /// * capacity needs to fit the layout size that the pointer was allocated
    ///   with.
    /// * the allocated size in bytes cannot exceed [`isize::MAX`]
    #[inline]
    #[must_use]
    pub(crate) unsafe fn from_nonnull_in(
        ptr: NonNull<T>,
        cap: Cap,
        alloc: A,
    ) -> Self {
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
    /// * `T` needs to have the same alignment as what `ptr` was allocated with.
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
    // FIXME should these be taking &mut self?
    #[inline]
    pub(crate) const fn as_non_null(&self) -> NonNull<T> {
        self.ptr.cast()
    }
    #[inline]
    pub(crate) const fn as_mut_ptr(&self) -> *mut T {
        self.as_non_null().as_ptr()
    }
    #[inline]
    pub(crate) const fn as_ptr(&self) -> *const T {
        self.ptr.as_ptr() as _
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
    #[cfg(test)]
    pub(crate) const fn raw_cap(&self) -> Cap {
        self.cap
    }
    #[inline]
    pub(crate) const fn allocator(&self) -> &A {
        &self.alloc
    }

    fn memory_layout(&self) -> Option<(NonNull<u8>, Layout)> {
        if self.cap == Cap::ZERO {
            None
        } else {
            // SAFETY:
            // * we allocated this chunk of memory so `unchecked_mul` and `size`
            //   rounded to the nearest power of two both cannot overflow
            //   `isize::MAX`.
            // * `align` is obtained through align_of so it is a power of two.
            unsafe {
                let size = size_of::<T>().unchecked_mul(self.cap.get());
                let layout =
                    Layout::from_size_align_unchecked(size, align_of::<T>());
                Some((self.ptr, layout))
            }
        }
    }
}

impl<T, A: Allocator> Drop for RawAtomicVec<T, A> {
    fn drop(&mut self) {
        if let Some((ptr, layout)) = self.memory_layout() {
            // SAFETY: we allocated this block of memory with this ptr and
            // this layout
            unsafe {
                self.alloc.deallocate(ptr, layout);
            }
        }
    }
}
