use std::num::NonZeroU32;
use std::time::Duration;

use std::sync::Arc;
use std::time::Instant;

use crossbeam_utils::atomic::AtomicCell;
use tokio::sync::Notify;
use tokio::time::sleep;

use crate::handle::Handle;

#[derive(Debug, Clone)]
pub(crate) struct IntervalNotifier {
	notifier: Arc<Notify>,
}

impl IntervalNotifier {
	pub(crate) async fn next(&self) {
		self.notifier.notified().await;
	}
}

struct Interval {
	last_started: Arc<AtomicCell<Instant>>,
	notifier: Arc<Notify>,
	handle: Handle,
}

impl Interval {
	fn new(d: Duration) -> (IntervalNotifier, Self) {
		let last_started = Arc::new(AtomicCell::new(Instant::now()));
		let notifier = Arc::new(Notify::new());

		let handle = tokio::spawn(task(d, last_started.clone(), notifier.clone()));

		let interval = Self {
			last_started,
			notifier: notifier.clone(),
			handle: Handle::from(handle),
		};

		let notifier = IntervalNotifier { notifier };

		(notifier, interval)
	}

	fn reset(&mut self, d: Duration) {
		self.handle.inner.abort();

		let handle = tokio::spawn(task(d, self.last_started.clone(), self.notifier.clone()));

		self.handle = Handle::from(handle);
	}
}

async fn task(d: Duration, last_started: Arc<AtomicCell<Instant>>, notifier: Arc<Notify>) {
	loop {
		let now = Instant::now();
		let passed = now - last_started.load();

		// TODO: replace with `Duration::saturating_sub` once stable
		let d = match d.checked_sub(passed) {
			Some(d) => d,
			None => Duration::from_nanos(0),
		};

		sleep(d).await;
		notifier.notify_waiters();

		last_started.store(Instant::now());
	}
}

/// An [AwarenessInterval] is an interval which multiplies its base interval
/// with the current awareness score to get the duration between intervals.
///
/// [AwarenessInterval] is used  for the `ping` and `gossip` intervals.
pub(super) struct AwarenessInterval {
	base_interval: Duration,
	inner: Interval,
}

impl AwarenessInterval {
	pub(super) fn new(base_interval: Duration) -> (IntervalNotifier, Self) {
		let (notifier, inner) = Interval::new(base_interval);
		let this = Self {
			base_interval,
			inner,
		};
		(notifier, this)
	}

	pub(super) fn update(&mut self, awareness: NonZeroU32) -> Duration {
		let d = awareness.get() * self.base_interval;
		self.inner.reset(d);
		d
	}
}

pub(super) struct SyncInterval {
	base_interval: Duration,
	scale: u32,
	inner: Interval,
}

impl SyncInterval {
	pub(super) fn new(base_interval: Duration, scale: NonZeroU32) -> (IntervalNotifier, Self) {
		let (notifier, inner) = Interval::new(base_interval);
		let this = Self {
			base_interval,
			scale: scale.into(),
			inner,
		};
		(notifier, this)
	}

	pub(super) fn update(&mut self, node_count: NonZeroU32) -> Duration {
		let node_count = node_count.get();

		let d = if node_count > self.scale {
			let node_count: f64 = node_count.into();
			let node_count = node_count.log2();

			let scale: f64 = self.scale.into();
			let scale = scale.log2();

			let multiplier = f64::ceil(node_count - scale) + 1.0;

			self.base_interval.mul_f64(multiplier)
		} else {
			self.base_interval
		};

		self.inner.reset(d);
		d
	}
}
