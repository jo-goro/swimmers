use std::collections::HashMap;
use std::num::NonZeroU32;
use std::time::Duration;

use tokio::sync::mpsc::{channel, Receiver, Sender};

use super::timer::{Output, Timer};

#[derive(Debug)]
enum PingTimer {
	/// Used for direct and indirect pings.
	Normal(Timer),
	/// Used for ping-requests. Uses 80% of the normal Timeout.
	Nack(Timer),
	/// Used for ping-requests after a [PingTimer::Nack]. Uses 20% of the normal Timeout.
	Grace(Timer),
}

impl PingTimer {
	const NORMAL_MUL: f64 = 1.00;
	const NACK_MUL: f64 = 0.80;
	const GRACE_MUL: f64 = 0.20;

	fn update(&mut self, normal_timeout: Duration, out: Output<u64>) {
		match self {
			PingTimer::Normal(timer) => timer.reset(normal_timeout.mul_f64(Self::NORMAL_MUL), out),
			PingTimer::Nack(timer) => timer.reset(normal_timeout.mul_f64(Self::NACK_MUL), out),
			PingTimer::Grace(timer) => timer.reset(normal_timeout.mul_f64(Self::GRACE_MUL), out),
		}
	}
}

#[derive(Debug)]
pub(super) struct PingTimers {
	base_timeout: Duration,
	map: HashMap<u64, PingTimer>,
	tx: Sender<u64>,

	awareness: NonZeroU32,
}

impl PingTimers {
	pub(super) fn new(base_timeout: Duration) -> (Receiver<u64>, Self) {
		let (tx, rx) = channel(1);
		let this = Self {
			base_timeout,
			map: HashMap::new(),
			tx,
			awareness: NonZeroU32::new(1).unwrap(),
		};
		(rx, this)
	}

	#[inline]
	fn calc_normal_timeout(&self) -> Duration {
		self.awareness.get() * self.base_timeout
	}

	#[inline]
	fn make_timer(&self, sequence: u64, multiplier: f64) -> Timer {
		let out = Output {
			value: sequence,
			tx: self.tx.clone(),
		};

		let d = self.calc_normal_timeout().mul_f64(multiplier);

		Timer::new(d, out)
	}

	pub(super) fn start_normal(&mut self, sequence: u64) {
		let timer = self.make_timer(sequence, PingTimer::NORMAL_MUL);
		let timer = PingTimer::Normal(timer);
		self.map.insert(sequence, timer);
	}

	pub(super) fn start_nack(&mut self, sequence: u64) {
		let timer = self.make_timer(sequence, PingTimer::NACK_MUL);
		let timer = PingTimer::Nack(timer);
		self.map.insert(sequence, timer);
	}

	pub(super) fn start_grace(&mut self, sequence: u64) {
		let timer = self.make_timer(sequence, PingTimer::GRACE_MUL);
		let timer = PingTimer::Grace(timer);
		self.map.insert(sequence, timer);
	}

	pub(super) fn remove(&mut self, sequence: &u64) {
		self.map.remove(sequence);
	}

	pub(super) fn update_awareness(&mut self, awareness: NonZeroU32) {
		self.awareness = awareness;

		let d = self.calc_normal_timeout();

		for (&sequence, ping) in self.map.iter_mut() {
			let out = Output {
				value: sequence,
				tx: self.tx.clone(),
			};

			ping.update(d, out);
		}
	}
}
