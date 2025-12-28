use {
    crate::{AtomicVec, error::VecFull},
    std::{
        alloc::{Allocator, Global},
        ops,
        sync::{MutexGuard, atomic::Ordering},
    },
};

pub struct AtomicVecGuard<'a, T, A: Allocator = Global> {
    pub(crate) _guard: MutexGuard<'a, ()>,
    pub(crate) vec: &'a AtomicVec<T, A>,
}

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
        self.vec.as_slice()
    }
    /// # Panics
    /// if the vec is full (i.e. capacity == len).
    pub fn push(&mut self, value: T) {
        // We locked the mutex so writes cannot happen.
        let len = self.vec.len.load(Ordering::Relaxed);
        let cap = self.vec.capacity();

        assert!(len < cap, "length overflow");

        // SAFETY: the ptr is still in the allocated block, even after add(len)
        unsafe {
            let dst = self.vec.as_non_null_ref().add(len);
            dst.write(value);
            self.vec.len.store(len + 1, Ordering::Release);
        }
    }
    /// # Errors
    /// Returns an error if the [`AtomicVec`] is full, i.e. `len == capacity`
    pub fn try_push(&mut self, value: T) -> Result<(), VecFull> {
        // We locked the mutex so writes cannot happen.
        let len = self.vec.len.load(Ordering::Relaxed);
        let cap = self.vec.capacity();

        if len >= cap {
            return Err(VecFull);
        }

        // SAFETY: the ptr is still in the allocated block, even after add(len)
        unsafe {
            let dst = self.vec.as_non_null_ref().add(len);
            dst.write(value);
        }
        self.vec.len.store(len + 1, Ordering::Release);

        Ok(())
    }
}
