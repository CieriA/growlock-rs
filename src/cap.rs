use std::mem::SizedTypeProperties as _;

/// `Capacity` of the [`AtomicVec`](crate::AtomicVec)
///
/// # Invariants
/// inner value must be <= [`isize::MAX`]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Cap(usize);
impl Cap {
    pub(crate) const ZERO: Self = Self(0);
    /// Returns `Cap(cap)`, or `Cap::ZERO` if `T` is a ZST.
    ///
    /// # Safety: `cap` must be <= [`isize::MAX`]
    #[inline]
    pub(crate) const unsafe fn new_unchecked<T>(cap: usize) -> Self {
        if T::IS_ZST { Self::ZERO } else { Self(cap) }
    }

    /// Returns `None` if `cap` > [`isize::MAX`],
    /// `Some(Cap::ZERO)` if `T` is a ZST or
    /// `Some(Cap(cap))` otherwise.
    #[inline]
    pub(crate) const fn try_new<T>(cap: usize) -> Option<Self> {
        const I_MAX: usize = isize::MAX as usize;
        match cap {
            // SAFETY: `cap` is in the correct range of values.
            0..I_MAX => Some(unsafe { Self::new_unchecked::<T>(cap) }),
            _ => None,
        }
    }
    /// Gets the inner `usize`.
    #[inline]
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}
