use std::num::NonZeroU32;

pub(crate) const MAX_NON_ZERO_U32: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(u32::MAX) };
pub(crate) const MIN_NON_ZERO_U32: NonZeroU32 = unsafe { NonZeroU32::new_unchecked(1) };
