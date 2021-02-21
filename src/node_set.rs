use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::mem::swap;
use std::net::SocketAddr;

use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::node::{Node, NodeState};

pub(crate) enum InsertionResult<'a> {
	Unchanged,
	Equal(&'a Node),
	Updated(&'a Node),
	Inserted(&'a Node),
}

/// An [Iterator] returning the [SocketAddr] for each [Node] **exactly once** in semi-random order.
#[derive(Debug)]
pub(crate) struct Iter<'a, R> {
	src: &'a mut NodeSet<R>,
	visited: HashSet<SocketAddr>,
	next_item: Option<SocketAddr>,
	active_nodes: usize,
}

impl<'a, R> Iterator for Iter<'a, R>
where
	R: Rng,
{
	type Item = SocketAddr;

	fn next(&mut self) -> Option<Self::Item> {
		loop {
			let addr = self
				.src
				.pop()
				.expect("`pop` must return `Some` at this point");

			if !self.src.contains(&addr) {
				continue;
			}

			if self.visited.insert(addr) {
				let mut next_item = Some(addr);
				swap(&mut next_item, &mut self.next_item);
				return next_item;
			}

			// return last addr in `self.next_item` and `None` after that once every addr has been visited.
			if self.visited.len() == self.active_nodes {
				let mut next_item = None;
				swap(&mut next_item, &mut self.next_item);
				return next_item;
			}
		}
	}
}

#[derive(Debug)]
pub(crate) struct NodeSet<R> {
	map: HashMap<SocketAddr, Node>,
	stack: Vec<SocketAddr>,

	rng: R,
}

impl<R> NodeSet<R>
where
	R: Rng,
{
	/// Returns an [Iterator] returning the [SocketAddr] for each [Node] **exactly once** in semi-random order.
	pub(crate) fn iter_unique_random_addrs<'a>(&'a mut self) -> Option<Iter<'a, R>> {
		let addr = loop {
			let addr = self.pop()?;

			if self.contains(&addr) {
				break addr;
			}
		};

		let (a, s, d, _) = self.counts();
		let active_nodes = a + s + d;

		let mut visited = HashSet::with_capacity(active_nodes);
		visited.insert(addr);

		Some(Iter {
			visited,
			src: self,
			next_item: Some(addr),
			active_nodes,
		})
	}

	/// Pops the next [SocketAddr] of the stack. Refills the stack if the last item has been popped off.
	/// Returns [None] if the stack is empty after refilling it.
	fn pop(&mut self) -> Option<SocketAddr> {
		loop {
			if let Some(addr) = self.stack.pop() {
				return Some(addr);
			}

			self.refill_stack();

			if self.stack.is_empty() {
				return None;
			}
		}
	}

	/// Refills and shuffles the internal random stack. Ignores nodes which left the cluster.
	fn refill_stack(&mut self) {
		let mut stack = Vec::with_capacity(self.map.len());

		for s in self.map.values().filter_map(|n| match n.state {
			NodeState::Left => None,
			_ => Some(n.addr),
		}) {
			stack.push(s);
		}

		stack.shuffle(&mut self.rng);
		self.stack = stack;
	}
}

impl Default for NodeSet<SmallRng> {
	fn default() -> Self {
		Self::new(SmallRng::from_entropy())
	}
}

impl<R> NodeSet<R> {
	pub(crate) fn new(rng: R) -> Self {
		Self {
			map: HashMap::new(),
			stack: Vec::new(),
			rng,
		}
	}

	/// Returns the total amount of nodes.
	///
	/// Use `counts` if you need the amount of nodes grouped by state.
	#[inline]
	pub(crate) fn len(&self) -> usize {
		self.map.len()
	}

	/// Returns `true` if the [NodeMap] contains the given [SocketAddr].
	#[inline]
	pub(crate) fn contains(&self, addr: &SocketAddr) -> bool {
		self.map.contains_key(addr)
	}

	pub(crate) fn insert(&mut self, node: Node) -> InsertionResult {
		match self.map.entry(node.addr) {
			Entry::Vacant(entry) => {
				let node = entry.insert(node);
				InsertionResult::Inserted(node)
			}
			Entry::Occupied(entry) => {
				let current = entry.into_mut();
				match Ord::cmp(&node.state, &current.state) {
					Ordering::Less => InsertionResult::Unchanged,
					Ordering::Equal => InsertionResult::Equal(current),
					Ordering::Greater => {
						*current = node;
						InsertionResult::Updated(current)
					}
				}
			}
		}
	}

	#[inline]
	pub(crate) fn get(&self, addr: &SocketAddr) -> Option<&Node> {
		self.map.get(addr)
	}

	#[inline]
	pub(crate) fn get_mut(&mut self, addr: &SocketAddr) -> Option<&mut Node> {
		self.map.get_mut(addr)
	}

	#[inline]
	pub(crate) fn remove(&mut self, addr: &SocketAddr) -> Option<Node> {
		self.map.remove(addr)
	}

	#[inline]
	pub(crate) fn get_map(&self) -> &HashMap<SocketAddr, Node> {
		self.map.borrow()
	}

	/// Returns the amount of nodes with each state. The amounts are ordered as follows:
	/// 1. [NodeState::Alive]
	/// 2. [NodeState::Suspect]
	/// 3. [NodeState::Dead]
	/// 4. [NodeState::Left]
	pub(crate) fn counts(&self) -> (usize, usize, usize, usize) {
		self.map
			.iter()
			.fold((0, 0, 0, 0), |(a, s, d, l), (_, data)| match data.state {
				NodeState::Alive(_) => (a + 1, s, d, l),
				NodeState::Suspect(_) => (a, s + 1, d, l),
				NodeState::Dead(_) => (a, s, d + 1, l),
				NodeState::Left => (a, s, d, l + 1),
			})
	}
}

#[cfg(test)]
mod tests {
	use std::net::{Ipv4Addr, SocketAddrV4};

	use super::*;

	use crate::node::{Node, NodeState};
	use rand::rngs::mock::StepRng;

	fn make_addr(port: u16) -> SocketAddr {
		SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), port))
	}

	#[test]
	fn pop_returns_none_if_map_is_empty() {
		let rng = StepRng::new(0, 0);
		let mut n = NodeSet::new(rng);
		assert_eq!(n.pop(), None);
	}

	#[test]
	fn pop_refills_the_stack() {
		let rng = StepRng::new(0, 0);
		let mut n = NodeSet::new(rng);

		let addr = make_addr(1);
		n.insert(Node {
			addr,
			state: NodeState::Alive(1),
			metadata: None,
		});

		n.refill_stack();

		assert_eq!(n.stack.len(), 1);
		assert_eq!(n.pop(), Some(addr));
		assert_eq!(n.stack.len(), 0);
		assert_eq!(n.pop(), Some(addr));
		assert_eq!(n.stack.len(), 0);
	}

	#[test]
	fn refill_stack_skips_left_nodes() {
		let rng = StepRng::new(0, 0);
		let mut n = NodeSet::new(rng);

		for i in 0..10 {
			n.insert(Node {
				addr: make_addr(i),
				state: if i % 2 == 0 {
					NodeState::Alive(i.into())
				} else {
					NodeState::Left
				},
				metadata: None,
			});
		}

		n.refill_stack();

		assert_eq!(n.stack.len(), 5);
	}

	#[test]
	fn iter_unique_random_addrs_returns_none_if_pop_returns_none() {
		let rng = StepRng::new(0, 0);
		let mut n = NodeSet::new(rng);

		assert!(n.iter_unique_random_addrs().is_none());

		let addr = make_addr(1);
		n.insert(Node {
			addr,
			state: NodeState::Alive(1),
			metadata: None,
		});

		n.refill_stack();

		n.remove(&addr);

		assert!(n.iter_unique_random_addrs().is_none());
	}

	#[test]
	fn iter_unique_random_addrs_only_returns_unique_adds() {
		let rng = StepRng::new(0, 0);
		let mut n = NodeSet::new(rng);

		for i in 0..10 {
			n.insert(Node {
				addr: make_addr(i),
				state: NodeState::Alive(i.into()),
				metadata: None,
			});
		}

		let iter = n.iter_unique_random_addrs().unwrap();
		let mut set = HashSet::with_capacity(10);

		for a in iter {
			set.insert(a);
		}

		assert_eq!(set.len(), 10);
	}

	fn insert_returns_correct_result() {
		let rng = StepRng::new(0, 0);
		let mut n = NodeSet::new(rng);

		let cases = vec![(0, 3), (1, 2), (3, 2), (2, 0), (3, 1)];

		for (i, result) in cases {
			let r = n.insert(Node {
				addr: make_addr(1),
				state: NodeState::Alive(i),
				metadata: None,
			});

			let r = match r {
				InsertionResult::Unchanged => 0,
				InsertionResult::Equal(n) if n.state.incarnation().unwrap() == i => 1,
				InsertionResult::Updated(n) if n.state.incarnation().unwrap() == i => 2,
				InsertionResult::Inserted(n) if n.state.incarnation().unwrap() == i => 3,
				_ => 100,
			};

			assert_eq!(r, result);
		}
	}
}
