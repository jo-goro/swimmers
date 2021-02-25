use std::convert::TryInto;
use std::num::{NonZeroU32, NonZeroUsize};

use interval::{AwarenessInterval, SyncInterval};
use ping::PingTimers;
use suspicion::{State, SuspicionTimers, TimeoutCalculator};
use tokio::sync::mpsc::Receiver;

mod interval;
mod ping;
mod suspicion;
mod timer;

pub(crate) use interval::IntervalNotifier;
pub(crate) use suspicion::KillRequest;

use crate::consts::MAX_NON_ZERO_U32;
use crate::SchedulerConfig;

pub(crate) struct SchedulerEvents {
	sync_notifier: IntervalNotifier,
	ping_notifier: IntervalNotifier,
	gossip_notifier: IntervalNotifier,

	suspicion_timeout: Receiver<KillRequest>,
	ping_timeout: Receiver<u64>,
}

pub(crate) enum SchedulerEvent {
	SyncInterval,
	PingInterval,
	GossipInterval,
	SuspicionTimeout(KillRequest),
	PingTimeout(u64),
}

impl SchedulerEvents {
	// TODO: use futures::Stream instead.
	pub(crate) async fn next(&mut self) -> SchedulerEvent {
		tokio::select! {
			_ = self.sync_notifier.next() => SchedulerEvent::SyncInterval,
			_ = self.ping_notifier.next() => SchedulerEvent::PingInterval,
			_ = self.gossip_notifier.next() => SchedulerEvent::GossipInterval,
			Some(k) = self.suspicion_timeout.recv() => SchedulerEvent::SuspicionTimeout(k),
			Some(i) = self.ping_timeout.recv() => SchedulerEvent::PingTimeout(i),
		}
	}
}

pub(crate) struct Scheduler {
	sync_interval: SyncInterval,
	ping_interval: AwarenessInterval,
	gossip_interval: AwarenessInterval,

	ping_timers: PingTimers,
	suspicion_timers: SuspicionTimers,
}

impl Scheduler {
	fn new(config: SchedulerConfig, node_count: NonZeroUsize) -> (SchedulerEvents, Self) {
		let (sync_notifier, sync_interval) =
			SyncInterval::new(config.sync.base_interval, config.sync.scale);
		let (ping_notifier, ping_interval) = AwarenessInterval::new(config.ping.base_interval);
		let (gossip_notifier, gossip_interval) =
			AwarenessInterval::new(config.base_gossip_interval);

		let tc = TimeoutCalculator::from(config.suspicion);
		let state = State {
			ping_interval: config.ping.base_interval,
			node_count: node_count.try_into().unwrap_or(MAX_NON_ZERO_U32),
		};

		let (suspicion_timeout, suspicion_timers) =
			SuspicionTimers::new(config.ping.base_interval, tc, state);
		let (ping_timeout, ping_timers) = PingTimers::new(config.ping.base_timeout);

		let e = SchedulerEvents {
			sync_notifier,
			ping_notifier,
			gossip_notifier,
			suspicion_timeout,
			ping_timeout,
		};

		let s = Self {
			sync_interval,
			ping_interval,
			gossip_interval,
			ping_timers,
			suspicion_timers,
		};

		(e, s)
	}

	fn update_awareness(&mut self, awareness: NonZeroU32) {
		self.gossip_interval.update(awareness);
		let ping_interval = self.ping_interval.update(awareness);

		self.ping_timers.update_awareness(awareness);
		self.suspicion_timers.update_ping_interval(ping_interval);
	}

	fn update_node_count(&mut self, node_count: NonZeroUsize) {
		let node_count = node_count.try_into().unwrap_or(MAX_NON_ZERO_U32);

		self.sync_interval.update(node_count);
		self.suspicion_timers.update_node_count(node_count);
	}
}
