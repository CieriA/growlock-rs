//! Capacity abstraction to permit its invariants.

use std::mem::SizedTypeProperties as _;

/// Representation of the `capacity`.
///
/// # Invariants
/// Inner value must be <= [`isize::MAX`]
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Cap(usize);
impl Cap {
    /// A `capacity` of zero (unallocated).
    pub(crate) const ZERO: Self = Self(0);

    /// Creates a new `capacity` without checking if it is <= [`isize::MAX`].
    /// The result is undefined if it is not.
    ///
    /// # Safety
    /// `cap` must be <= [`isize::MAX`]
    #[inline]
    pub(crate) const unsafe fn new_unchecked<T>(cap: usize) -> Self {
        if T::IS_ZST { Self::ZERO } else { Self(cap) }
    }

    /// Creates a new `capacity` if it is <= [`isize::MAX`]
    ///
    /// if `T` is a ZST, this returns a capacity of zero.
    #[inline]
    pub(crate) const fn new<T>(cap: usize) -> Option<Self> {
        const I_MAX: usize = isize::MAX as usize;
        match cap {
            _ if T::IS_ZST => Some(Cap::ZERO),
            // SAFETY: `cap` is in the correct range of values.
            0..I_MAX => Some(unsafe { Self::new_unchecked::<T>(cap) }),
            _ => None,
        }
    }
    /// Returns the `capacity` as a primitive value.
    #[inline]
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}
