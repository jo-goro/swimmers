use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::num::NonZeroUsize;

use thiserror::Error;

#[derive(Debug)]
pub(crate) enum Ping {
	/// A direct ping to a node.
	Direct(SocketAddr),
	/// An indirect ping to a node. The [HashSet] collects the [SocketAddr] of nodes which send a `nack` back.
	Indirect(SocketAddr, HashSet<SocketAddr>),
	/// A request to ping another node. [Request] specifies the `sequence`-number
	/// and the address of the node that requested the ping.
	Request(RequestSource, bool),
}
#[derive(Debug)]
pub(crate) enum FailResult {
	/// Signals the failure of a direct ping and orders the caller to do an indirect ping.
	DoIndirect(PingTarget),
	/// Signals a `nack`-timeout for a [Ping::Request] and orders the caller to send a nack to the
	/// node specified in the [RequestSource]. Invoked when 80% of the timeout has passed.
	SendNack(RequestSource),
	/// Signals the failure of a [Ping::Request].
	RequestFailed(RequestSource),
	/// Signals the failure of an indirect ping. Contains the address of the node which should now be suspected
	/// and a set of the [SocketAddr] of the nodes which returned nacks.
	NodeFailed(SocketAddr, HashSet<SocketAddr>),
}

macro_rules! impl_reqs {
	($t:ident) => {
		#[derive(Debug, PartialEq, Eq, Copy, Clone)]
		pub struct $t {
			pub sequence: u64,
			pub addr: SocketAddr,
		}
	};
}

impl_reqs!(PingTarget);
impl_reqs!(PingRequestTarget);
impl_reqs!(RequestSource);

#[derive(Debug, Error)]
#[error("node `{0}` gets currently pinged")]
pub(crate) struct NodeAlreadyPingedError(SocketAddr);

#[derive(Debug, Default)]
pub(crate) struct PingStore {
	sequence: u64,
	pings: HashMap<u64, Ping>,
	/// Stores the addresses of the current direct and indirect pings.
	current: HashSet<SocketAddr>,
}

impl PingStore {
	fn new() -> Self {
		Default::default()
	}

	/// Returns the current `sequence`-number and increments the counter.
	fn next_sequence(&mut self) -> u64 {
		let result = self.sequence;
		self.sequence += 1;
		result
	}

	#[inline]
	fn current_sequence(&self) -> u64 {
		self.sequence
	}

	#[inline]
	fn current_pings(&self) -> &HashSet<SocketAddr> {
		&self.current
	}

	pub(crate) fn ping(&mut self, addr: SocketAddr) -> Result<PingTarget, NodeAlreadyPingedError> {
		if !self.current.insert(addr) {
			return Err(NodeAlreadyPingedError(addr));
		}

		let sequence = self.next_sequence();
		let ping = Ping::Direct(addr);

		self.pings.insert(sequence, ping);

		Ok(PingTarget { sequence, addr })
	}

	pub(crate) fn ping_request(
		&mut self,
		source: RequestSource,
		target: SocketAddr,
	) -> PingRequestTarget {
		let sequence = self.next_sequence();
		let ping = Ping::Request(source, false);

		self.pings.insert(sequence, ping);

		let request = PingRequestTarget {
			sequence,
			addr: target,
		};

		request
	}

	/// Returns [Some] [Ping] for the given `sequence`-number which has been `acked`.
	///
	/// Returns [None] if the `sequence`-number has already been `acked` or failed.
	pub(crate) fn ack(&mut self, sequence: &u64) -> Option<Ping> {
		let ping = self.pings.remove(sequence)?;

		match &ping {
			Ping::Direct(addr) | Ping::Indirect(addr, _) => {
				assert!(self.current.remove(addr));
			}
			_ => {}
		}

		Some(ping)
	}

	/// Registers a `nack` from a node identified by its [SocketAddr] for a given `sequence`-number and
	/// returns [Some] amount of recieved `nacks`. Returns [None] if there is currently no active ping
	/// with the given `sequence`-number or the `sequence`-number was already `nacked` by the same node.
	pub(crate) fn nack(&mut self, sequence: u64, from: SocketAddr) -> Option<NonZeroUsize> {
		if let Entry::Occupied(mut ping) = self.pings.entry(sequence) {
			if let Ping::Indirect(_, ref mut nacks) = ping.get_mut() {
				if nacks.insert(from) {
					let count = nacks.len();
					return NonZeroUsize::new(count); // Always Some.
				}
			}
		}

		None
	}

	/// Returns [Some] amount of `nacks` recived for a given `sequence`-number.
	///
	/// [None] will be returned if the `sequence`-number cannot be found, or the ping is not an indirect ping.
	fn nack_count(&self, sequence: &u64) -> Option<usize> {
		let ping = self.pings.get(sequence)?;
		match ping {
			Ping::Indirect(_, nacks) => Some(nacks.len()),
			_ => None,
		}
	}

	pub(crate) fn fail(&mut self, sequence: u64) -> Option<FailResult> {
		let ping = self.pings.remove(&sequence)?;

		match ping {
			Ping::Request(source, true) => Some(FailResult::RequestFailed(source)),
			Ping::Request(source, false) => {
				let ping = Ping::Request(source, true);

				self.pings.insert(sequence, ping);

				Some(FailResult::SendNack(source))
			}
			Ping::Direct(addr) => {
				let sequence = self.next_sequence();
				let ping = Ping::Indirect(addr, HashSet::new());
				self.pings.insert(sequence, ping);

				let target = PingTarget { addr, sequence };
				Some(FailResult::DoIndirect(target))
			}
			Ping::Indirect(addr, nacks) => {
				assert!(self.current.remove(&addr));
				Some(FailResult::NodeFailed(addr, nacks))
			}
		}
	}

	#[inline]
	pub(crate) fn clear(&mut self) {
		self.pings.clear();
		self.current.clear();
	}

	/// Returns the currently ongoing pings in the order of:
	/// 1. `Direct`
	/// 2. `Indirect`
	/// 3. `Request`
	pub(crate) fn pingcounts(&self) -> (usize, usize, usize) {
		self.pings
			.values()
			.fold((0, 0, 0), |(d, i, r), ping| match ping {
				Ping::Direct(_) => (d + 1, i, r),
				Ping::Indirect(_, _) => (d, i + 1, r),
				Ping::Request(_, _) => (d, i, r + 1),
			})
	}
}

#[cfg(test)]
mod tests {
	use std::convert::TryInto;

	use super::*;

	fn addr(port: u16) -> SocketAddr {
		format!("127.0.0.1:{}", port).parse().unwrap()
	}

	#[test]
	fn pingcount() {
		let mut p = PingStore::new();

		let cases = vec![
			((0, 0, 0), vec![]),
			((1, 0, 0), vec![0]),
			((0, 1, 0), vec![1]),
			((1, 1, 1), vec![2, 0, 1]),
			((1, 2, 1), vec![1, 0, 1, 2]),
			((0, 0, 0), vec![]),
		];

		for (expected, inserts) in cases {
			for (seq, kind) in inserts
				.into_iter()
				.enumerate()
				.map(|(seq, kind)| (seq.try_into().unwrap(), kind))
			{
				let ping = match kind {
					0 => Ping::Direct(addr(1)),
					1 => Ping::Indirect(addr(1), HashSet::new()),
					2 => Ping::Request(
						RequestSource {
							addr: addr(1),
							sequence: 0,
						},
						true,
					),
					_ => unreachable!(),
				};

				p.pings.insert(seq, ping);
			}

			assert_eq!(expected, p.pingcounts());
			p.clear();
		}
	}

	fn cannot_fail_wrong_sequence() {
		let mut p = PingStore::new();
		assert!(p.fail(0).is_none());
	}

	fn nack() {
		let mut p = PingStore::new();
		assert!(p.nack(0, addr(1)).is_none());

		let result = p.ping(addr(1)).unwrap();
		assert_eq!(result.addr, addr(1));
		assert_eq!(result.sequence, 0);
		assert!(p.nack(0, addr(1)).is_none());

		let result = p.fail(0).unwrap();
		assert!(matches!(result, FailResult::DoIndirect(target) if target.sequence == 1));
		assert_eq!(p.pingcounts(), (0, 1, 0));
		assert!(p.nack(0, addr(1)).is_none());

		assert_eq!(p.nack(1, addr(2)).unwrap().get(), 2);
		assert_eq!(p.nack(1, addr(2)).unwrap().get(), 2);
	}

	#[test]
	fn ping_and_fail() {
		let mut p = PingStore::new();

		let result = p.ping(addr(1)).unwrap();
		assert_eq!(result.addr, addr(1));
		assert_eq!(result.sequence, 0);

		let result = p.ping(addr(1)).unwrap_err();
		assert_eq!(result.0, addr(1));

		let result = p.ping(addr(2)).unwrap();
		assert_eq!(result.addr, addr(2));
		assert_eq!(result.sequence, 1);

		assert_eq!(p.pingcounts(), (2, 0, 0));

		let result = p.fail(0).unwrap();
		assert!(matches!(result, FailResult::DoIndirect(target) if target.sequence == 2));
		assert_eq!(p.pingcounts(), (1, 1, 0));

		let result = p.fail(2).unwrap();
		assert!(matches!(result, FailResult::NodeFailed(_, _)));
		assert_eq!(p.pingcounts(), (1, 0, 0));
	}

	#[test]
	fn ping_req_and_fail() {
		let mut p = PingStore::new();

		p.ping_request(
			RequestSource {
				sequence: 0,
				addr: addr(1),
			},
			addr(100),
		);

		assert_eq!(p.pingcounts(), (0, 0, 1));

		let result = p.fail(0).unwrap();
		assert!(matches!(result, FailResult::SendNack(_)));
		assert_eq!(p.pingcounts(), (0, 0, 1));

		let result = p.fail(0).unwrap();
		assert!(matches!(result, FailResult::RequestFailed(_)));
		assert_eq!(p.pingcounts(), (0, 0, 0));

		assert!(p.fail(0).is_none())
	}
}
