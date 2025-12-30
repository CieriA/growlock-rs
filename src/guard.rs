#[cfg(not(loom))]
use std::sync::{MutexGuard, atomic::Ordering};

#[cfg(loom)]
use loom::sync::{MutexGuard, atomic::Ordering};
use {
    crate::{GrowLock, error::LengthError},
    std::{
        alloc::{Allocator, Global},
        ops,
    },
};

/// RAII structure used to release the exclusive write access of a lock
/// when dropped.
///
/// This structure is created by the [`write`][write] and
/// [`try_write`][try_write] method on [`GrowLock`]
///
/// [write]: GrowLock::write
/// [try_write]: GrowLock::try_write
pub struct GrowGuard<'lock, T, A: Allocator = Global> {
    lock: &'lock GrowLock<T, A>,
    _guard: MutexGuard<'lock, ()>,
}

impl<T, A: Allocator> ops::Deref for GrowGuard<'_, T, A> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl<'lock, T, A: Allocator> GrowGuard<'lock, T, A> {
    #[inline]
    #[must_use]
    pub(super) const fn new(
        lock: &'lock GrowLock<T, A>,
        guard: MutexGuard<'lock, ()>,
    ) -> Self {
        Self {
            lock,
            _guard: guard,
        }
    }
    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.lock.as_slice()
    }
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
        self.lock.capacity()
    }
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        // We locked the mutex so writes cannot happen.
        self.lock.len.load(Ordering::Relaxed)
    }
    /// # Panics
    /// Panics if `self.is_full()`.
    pub fn push(&mut self, value: T) {
        let len = self.len();
        let cap = self.capacity();

        assert!(len < cap, "length overflow");

        // SAFETY: the ptr is still in the allocated block, even after
        // add(len)
        unsafe {
            let dst = self.lock.as_non_null_ref().add(len);
            dst.write(value);
            self.lock.len.store(len + 1, Ordering::Release);
        }
    }
    /// # Errors
    /// Returns an error if `self.is_full()`.
    pub fn try_push(&mut self, value: T) -> Result<(), LengthError> {
        // We locked the mutex so writes cannot happen.
        let len = self.lock.len.load(Ordering::Relaxed);
        let cap = self.lock.capacity();

        if len >= cap {
            return Err(LengthError);
        }

        // SAFETY: the ptr is still in the allocated block, even after
        // add(len)
        unsafe {
            let dst = self.lock.as_non_null_ref().add(len);
            dst.write(value);
        }
        self.lock.len.store(len + 1, Ordering::Release);

        Ok(())
    }
}

impl<T, A: Allocator> Extend<T> for GrowGuard<'_, T, A> {
    /// Extends the [`GrowLock<T>`] with the contents of an iterator.
    ///
    /// # Panics
    /// This panics if the iterator has more elements than
    /// `self.capacity() - self.len()` (i.e. pushing all the
    /// elements would overflow `self.capacity()`.
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        for elem in iter {
            self.push(elem);
        }
    }
}
