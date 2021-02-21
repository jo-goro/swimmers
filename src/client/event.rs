use std::io;
use std::net::SocketAddr;
use std::num::NonZeroU32;

use crate::Node;

/// The cause why the node update event handler was invoked.
pub enum Cause {
	/// An update about the state of the node has been received.
	Update,
	/// The node could not be reached and has therefore been suspected.
	///
	/// This cause is always accompanied by the [NodeState::Suspect].
	Suspicion,
	/// The node could not be reached and the suspicion period is over.
	///
	/// This cause is always accompanied by the [NodeState::Dead].
	Death,
}

#[allow(unused_variables)] // The default impl causes warnings and prefixing the parameters with `_` looks bad in the docs.
pub trait EventHandler {
	/// Invoked when the node awareness score changes.
	fn awareness(&mut self, awareness: NonZeroU32, max: NonZeroU32) {}

	/// Invoked when the state of a node changes.
	fn node(&mut self, node: &Node, cause: Cause) {}

	/// Invoked when a node gets removed.
	fn removed(&mut self, node: Node) {}

	/// Invoked before gossiping.
	fn gossip(&mut self, addr: &[SocketAddr]) {}

	/// Invoked before a sync.
	fn sync(&mut self, addr: &SocketAddr) {}

	/// Invoked if a sync failed.
	fn sync_failed(&mut self, addr: &SocketAddr, err: io::Error) {}

	/// Invoked when an `ack` has been received.
	fn ack(&mut self, target: &SocketAddr) {}

	/// Invoked when an indirect `ack` has been received.
	fn indirect_ack(&mut self, target: &SocketAddr, from: &SocketAddr) {}

	/// Invoked when a `nack` has been received.
	fn nack(&mut self, target: &SocketAddr, from: &SocketAddr) {}

	/// Invoked when a ping has been received.
	fn received_ping(&mut self, addr: &SocketAddr) {}

	/// Invoked when a node gets pinged.
	fn ping(&mut self, addr: &SocketAddr) {}

	/// Invoked when a node gets indirectly pinged.
	fn indirect_ping(&mut self, target: &SocketAddr, executors: &[SocketAddr]) {}

	/// Invoked when an indirect ping has been requested.
	fn ping_request(&mut self, target: &SocketAddr, requestor: &SocketAddr) {}

	/// Invoked when this node was suspected.
	fn suspected(&mut self, suspector: &SocketAddr) {}

	/// Invoked when this node was declared dead.
	fn declared_dead(&mut self, declared_by: &SocketAddr) {}

	/// Invoked when this node is preparing to leave the cluster.
	fn leaving(&mut self) {}

	/// Invoked when this node has left the cluster.
	fn left(&mut self) {}

	/// Invoked when this node was frocefully stopped by dropping the handle.
	fn stopped(&mut self) {}
}

/// An implementation of [EventHandler] which does not handle any events.
pub struct NullEventHandler;
impl EventHandler for NullEventHandler {}
