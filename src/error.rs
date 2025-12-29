use {std::alloc::Layout, thiserror::Error};

/// Error type for `try_with_capacity` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum TryReserveError {
    #[error("memory allocation failed because capacity exceeded maximum")]
    CapacityOverflow,
    #[error("memory allocation failed because allocator returned an error")]
    AllocError(Layout),
}
impl From<Layout> for TryReserveError {
    #[inline]
    fn from(e: Layout) -> Self {
        Self::AllocError(e)
    }
}

/// Error type for `try_push` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Error)]
#[error("tried to push to the `GrowLock`, but the `GrowLock` is already full")]
pub struct LengthError;
