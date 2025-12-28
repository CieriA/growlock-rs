use {
    crate::AtomicVec,
    std::{
        alloc::{Allocator, Global},
        ops, slice,
        sync::{MutexGuard, atomic::Ordering},
    },
};

pub struct AtomicVecGuard<'a, T, A: Allocator = Global> {
    pub(crate) _guard: MutexGuard<'a, ()>,
    pub(crate) vec: &'a AtomicVec<T, A>,
}
// FIXME Does this make sense?
impl<T, A: Allocator> ops::Deref for AtomicVecGuard<'_, T, A> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl<T, A: Allocator> AtomicVecGuard<'_, T, A> {
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        // SAFETY:
        // 1. `self.as_ptr()` is valid for reads for `self.len` elements of type
        //    `T`.
        // This is guaranteed because `RawAtomicVec` allocates at least
        // `self.cap` elements, and `self.len <= self.cap` is maintained
        // as an invariant.
        // 2. The memory is initialized for the range `0..self.len`.
        // 3. The entire memory range is contained within a single allocated
        //    object.
        // 4. The pointer is properly aligned for type `T`.
        unsafe { slice::from_raw_parts(self.vec.as_ptr(), self.vec.len()) }
    }
    /// # Panics
    /// if the vec is full (i.e. capacity == len).
    pub fn push(&self, value: T) {
        // We locked the mutex so writes cannot happen.
        let len = self.vec.len.load(Ordering::Relaxed);
        let cap = self.vec.capacity();

        assert!(len < cap, "length overflow");

        // SAFETY: the ptr is still in the allocated block, even after add(len)
        unsafe {
            let dst = self.vec.as_non_null().add(len);
            dst.write(value);
            self.vec.len.store(len + 1, Ordering::Release);
        }
    }
}
