use std::convert::TryInto;
use std::num::NonZeroU32;
use std::sync::atomic::AtomicU32;

/// A local health awareness component which assumes values in the range of `[1, max]`.
/// The counter always starts at `1`.
///
/// The local health awareness component, as specified by the *Lifeguard*-paper, uses values in the range
/// of `[0, max)`, unlike this implementation, which starts at `1` and has an inclusive upper bound.
///	This change was made because the awareness score is only used in a context where it must be at least `1`.
/// The component specified in the paper increments the score by `1` for each usecase (multiplier for the ping interval and timeout).
/// To avoid this unneeded addition, the lower bound was raised to `1` and the upper bound was made inclusive.
pub(crate) struct Awareness {
	max: u32,
	score: u32,
}

impl Default for Awareness {
	/// Creates a new [Awareness]-counter with `max == 9`.
	fn default() -> Self {
		Self::new(NonZeroU32::new(9).unwrap())
	}
}

impl Awareness {
	/// Creates a new [Awareness]-counter with a range of `[1, max]`.
	/// The counter starts at `1`.
	pub(crate) fn new(max: NonZeroU32) -> Self {
		Self {
			max: max.into(),
			score: 1,
		}
	}

	/// Increments the `score`.
	pub(crate) fn increment(&mut self) -> NonZeroU32 {
		if self.score < self.max {
			self.score += 1;
		}
		self.score()
	}

	/// Decrements the `score`.
	pub(crate) fn decrement(&mut self) -> NonZeroU32 {
		if self.score > 1 {
			self.score -= 1;
		}
		self.score()
	}

	/// Returns the current awareness score.
	#[inline]
	pub fn score(&self) -> NonZeroU32 {
		self.score.try_into().unwrap()
	}

	/// Returns the max awareness score.
	#[inline]
	pub fn max(&self) -> NonZeroU32 {
		self.max.try_into().unwrap()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn awareness() {
		let mut a = Awareness::default();

		let cases = vec![
			(-1i32, 1u32),
			(-10, 1),
			(1, 2),
			(-1, 1),
			(20, 9),
			(-1, 8),
			(-1, 7),
			(-1, 6),
			(-1, 5),
			(-1, 4),
			(-1, 3),
			(-1, 2),
			(-1, 1),
			(-1, 1),
		];

		for (rounds, result) in cases {
			for _ in 0..rounds.abs() {
				if rounds.is_negative() {
					a.decrement();
				} else {
					a.increment();
				};
			}

			assert_eq!(a.score().get(), result);
		}
	}
}
