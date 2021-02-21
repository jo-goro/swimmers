use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use std::net::SocketAddr;
use std::num::NonZeroUsize;

#[derive(Debug)]
pub(crate) struct Suspicion {
	pub(crate) incarnation: u64,
	pub(crate) suspectors: HashSet<SocketAddr>,
}

pub(crate) enum SuspicionResult {
	/// A new suspicion has been added.
	New,
	/// The suspicion was updated due to receiving a higher incrantion number.
	Reset,
	/// The suspicion has been updated. Contains the number of suspectors.
	Update(NonZeroUsize),
}

impl SuspicionResult {
	fn suspicions(&self) -> NonZeroUsize {
		match self {
			SuspicionResult::Reset | SuspicionResult::New => NonZeroUsize::new(1).unwrap(),
			SuspicionResult::Update(u) => *u,
		}
	}
}

#[derive(Debug, Default)]
pub(crate) struct Suspecions {
	suspicions: HashMap<SocketAddr, Suspicion>,
}

impl Suspecions {
	pub(crate) fn new() -> Self {
		Self::default()
	}

	pub(crate) fn suspect(
		&mut self,
		addr: SocketAddr,
		incarnation: u64,
		suspector: SocketAddr,
	) -> Option<SuspicionResult> {
		let result = match self.suspicions.entry(addr) {
			Entry::Vacant(entry) => {
				let mut suspectors = HashSet::with_capacity(1);
				suspectors.insert(suspector);

				let suspicion = Suspicion {
					incarnation,
					suspectors,
				};
				entry.insert(suspicion);

				SuspicionResult::New
			}
			Entry::Occupied(entry) => {
				let suspicion = entry.into_mut();
				match incarnation.cmp(&suspicion.incarnation) {
					Ordering::Less => return None,
					Ordering::Greater => {
						suspicion.suspectors.clear();
						suspicion.suspectors.insert(suspector);

						suspicion.incarnation = incarnation;

						SuspicionResult::Reset
					}
					Ordering::Equal => {
						suspicion.suspectors.insert(suspector);

						let count = suspicion.suspectors.len();
						let count = NonZeroUsize::new(count).unwrap();

						SuspicionResult::Update(count)
					}
				}
			}
		};

		Some(result)
	}

	#[inline]
	pub(crate) fn get(&mut self, addr: &SocketAddr) -> Option<&Suspicion> {
		self.suspicions.get(addr)
	}

	#[inline]
	pub(crate) fn remove(&mut self, addr: &SocketAddr) -> Option<Suspicion> {
		self.suspicions.remove(addr)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn suspect() {
		fn addr(port: u16) -> SocketAddr {
			format!("127.0.0.1:{}", port).parse().unwrap()
		}

		let mut s = Suspecions::new();

		let result = s.suspect(addr(1), 1, addr(1)).unwrap();
		assert!(matches!(result, SuspicionResult::New));
		assert!(s.get(&addr(1)).is_some());

		let result = s.suspect(addr(1), 1, addr(2)).unwrap();
		assert!(matches!(result, SuspicionResult::Update(i) if i.get() == 2));
		assert!(s.get(&addr(1)).is_some());

		let result = s.suspect(addr(1), 0, addr(2));
		assert!(result.is_none());
		assert!(s.get(&addr(1)).is_some());

		let result = s.suspect(addr(1), 2, addr(2)).unwrap();
		assert!(matches!(result, SuspicionResult::Reset));
		assert!(s.get(&addr(1)).is_some());

		let result = s.remove(&addr(1)).unwrap();
		assert_eq!(result.incarnation, 2);
		assert_eq!(result.suspectors.len(), 1);
		assert!(s.get(&addr(1)).is_none());

		let result = s.remove(&addr(1));
		assert!(result.is_none());
	}
}
