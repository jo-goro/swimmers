use std::cmp::Ordering;
use std::net::SocketAddr;

use thiserror::Error;

use NodeState::{Alive, Dead, Left, Suspect};

#[derive(Debug, Error)]
#[error("cannot suspect nodes which are not in the state `alive`")]
pub(crate) struct SuspectError;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("cannot kill nodes which are not in the state `alive` or `suspect`")]
pub(crate) struct KillError;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("cannot leave more than once")]
pub(crate) struct LeaveError;

/// The state of a [Node].
///
/// The states [Alive], [Suspect] and [Dead] are the three states described in the *SWIM*-paper.
/// Each of these states carries the current incarnation number of the node, which can only
/// be incremented by the node itself. The incarnation number will be incremented, if a node
/// refutes a suspicion or if it updates its metadata.
///
/// An additional [Left] state has been added to indicate that a Node has willingly left the cluster.
///
/// Two states can be ordered by the following rules:
/// 1. [Left] is greater than any other state.
/// 2. A state with a higher incarnation number is greater than one with a lower number.
/// 3. If the incarnation numbers are equal then `Dead > Suspect > Alive`.
///
/// # Example
/// ```
/// use swimmers::NodeState;
///
/// assert!(NodeState::Left > NodeState::Alive(1));
/// assert!(NodeState::Alive(2) > NodeState::Alive(1));
/// assert!(NodeState::Dead(1) > NodeState::Suspect(1));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeState {
	Alive(u64),
	Suspect(u64),
	Dead(u64),
	Left,
}

impl NodeState {
	/// Returns [Some] incarnation number or [None] if the state of the state is [Left].
	/// # Example
	/// ```
	/// use swimmers::NodeState;
	///
	/// assert_eq!(NodeState::Alive(1).incarnation(), Some(1));
	/// assert_eq!(NodeState::Left.incarnation(), None);
	/// ```
	pub fn incarnation(&self) -> Option<u64> {
		match *self {
			Alive(i) | Suspect(i) | Dead(i) => Some(i),
			Left => None,
		}
	}

	/// Sets `self` to [Suspect] if [Dead], keeping the incarnation number.
	/// Returns `true` if the state was changed.
	pub(crate) fn suspect_if_dead(&mut self) -> bool {
		if let Dead(i) = *self {
			*self = Suspect(i);
			true
		} else {
			false
		}
	}

	/// Sets the state to [Suspect], keeping the incarnation number.
	///
	/// This action only works if the current state is [Alive].
	/// An error will be returned if the current state is not [Alive].
	pub(crate) fn suspect(&mut self) -> Result<(), SuspectError> {
		if let Alive(i) = *self {
			*self = Suspect(i);
			Ok(())
		} else {
			Err(SuspectError)
		}
	}

	/// Sets the state to [Dead], keeping the incarnation number.
	///
	/// This action only works if the current state is either [Alive] or [Suspect].
	/// Otherwise an error will be returned.
	pub(crate) fn kill(&mut self) -> Result<(), KillError> {
		match *self {
			Dead(_) | Left => Err(KillError),
			Alive(i) | Suspect(i) => {
				*self = Dead(i);
				Ok(())
			}
		}
	}

	pub(crate) fn leave(&mut self) -> Result<(), LeaveError> {
		if let Left = *self {
			Err(LeaveError)
		} else {
			*self = Left;
			Ok(())
		}
	}

	/// Sets the state to [Alive] and increments the incrantion number.
	/// Does nothing if [Left].
	pub(crate) fn reincarnate(&mut self) {
		if let Some(i) = self.incarnation() {
			*self = Alive(i + 1);
		}
	}
}

impl Ord for NodeState {
	fn cmp(&self, other: &Self) -> Ordering {
		use Ordering::*;

		match (self, other) {
			(i, j) if i == j => Equal,
			(Alive(i), Alive(j)) if i > j => Greater,
			(Alive(i), Suspect(j)) if i > j => Greater,
			(Alive(i), Dead(j)) if i > j => Greater,
			(Suspect(i), Alive(j)) if i >= j => Greater,
			(Suspect(i), Suspect(j)) if i > j => Greater,
			(Suspect(i), Dead(j)) if i > j => Greater,
			(Dead(i), Alive(j)) if i >= j => Greater,
			(Dead(i), Suspect(j)) if i >= j => Greater,
			(Dead(i), Dead(j)) if i > j => Greater,
			(Left, _) => Greater,
			_ => Less,
		}
	}
}

impl PartialOrd for NodeState {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(Ord::cmp(self, other))
	}
}

/// A node in a swim cluster.
#[derive(Debug, Clone)]
pub struct Node {
	/// The address of the node.
	pub addr: SocketAddr,
	/// Current state of the node.
	pub state: NodeState,
	/// Optional metadata of a node.
	pub metadata: Option<Box<[u8]>>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn node_state_cmp() {
		use Ordering::*;

		let cases = vec![
			(Alive(1), Alive(1), Equal),
			(Suspect(1), Suspect(1), Equal),
			(Dead(1), Dead(1), Equal),
			(Left, Left, Equal),
			(Alive(2), Alive(1), Greater),
			(Alive(2), Suspect(1), Greater),
			(Alive(2), Dead(1), Greater),
			(Suspect(2), Suspect(1), Greater),
			(Suspect(1), Alive(1), Greater),
			(Suspect(2), Dead(1), Greater),
			(Dead(2), Dead(1), Greater),
			(Dead(1), Alive(1), Greater),
			(Dead(1), Suspect(1), Greater),
			(Left, Alive(1), Greater),
			(Left, Suspect(1), Greater),
			(Left, Dead(1), Greater),
			(Alive(1), Alive(2), Less),
			(Alive(1), Suspect(1), Less),
			(Alive(1), Dead(1), Less),
			(Alive(1), Left, Less),
			(Suspect(1), Alive(2), Less),
			(Suspect(1), Suspect(2), Less),
			(Suspect(1), Dead(1), Less),
			(Suspect(1), Left, Less),
			(Dead(1), Alive(2), Less),
			(Dead(1), Suspect(2), Less),
			(Dead(1), Dead(2), Less),
			(Dead(1), Left, Less),
		];

		for (ref i, ref j, result) in cases {
			assert_eq!(Ord::cmp(i, j), result);
		}
	}

	#[test]
	fn suspect_if_dead() {
		let cases = vec![
			(Left, Left, false),
			(Alive(1), Alive(1), false),
			(Suspect(1), Suspect(1), false),
			(Dead(1), Suspect(1), true),
		];

		for (mut before, after, changed) in cases {
			assert_eq!(before.suspect_if_dead(), changed);
			assert_eq!(before, after);
		}
	}

	#[test]
	fn reincarnate() {
		let cases = vec![
			(Alive(1), Alive(2)),
			(Suspect(1), Alive(2)),
			(Dead(1), Alive(2)),
			(Left, Left),
		];

		for (mut before, after) in cases {
			before.reincarnate();
			assert_eq!(before, after);
		}
	}
}
