use std::collections::HashMap;
use std::convert::TryInto;
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::consts::{MAX_NON_ZERO_U32, MIN_NON_ZERO_U32};
use crate::SuspicionConfig;

use super::timer::{Output, Timer};

#[derive(Debug, Clone, Copy)]
pub(crate) struct KillRequest {
	pub(crate) addr: SocketAddr,
	pub(crate) incarnation: u64,
}

#[derive(Debug)]
pub(crate) struct State {
	pub(crate) ping_interval: Duration,
	pub(crate) node_count: NonZeroU32,
}

#[derive(Debug)]
pub(crate) struct TimeoutCalculator {
	alpha: f64,
	beta: f64,
	k: NonZeroU32,
}

impl From<SuspicionConfig> for TimeoutCalculator {
	fn from(config: SuspicionConfig) -> Self {
		Self {
			alpha: config.alpha,
			beta: config.beta,
			k: config.k,
		}
	}
}

impl TimeoutCalculator {
	fn min_max(&self, state: &State) -> (Duration, Duration) {
		let node_count: f64 = state.node_count.get().into();
		let scale = f64::max(1.0, self.alpha * node_count.log10());
		let min = state.ping_interval.mul_f64(scale);

		let max = min.mul_f64(self.beta);

		(min, max)
	}

	fn timeout(&self, min: Duration, max: Duration, c: NonZeroU32) -> Duration {
		let min = min.as_secs_f64();
		let max = max.as_secs_f64();

		let c = c.get();
		let k = self.k.get() + 1; // + 1 to ensure the divisor is never 0.

		let c: f64 = c.into();
		let k: f64 = k.into();

		let frac = c.log10() / k.log10();
		let f = max - (max - min) * frac;

		let duration_secs = f64::max(min, f);
		let duration_millis = (duration_secs * 1000f64).floor() as u64; // Round to millis.

		Duration::from_millis(duration_millis)
	}
}

pub(crate) struct SuspicionTimers {
	base_timeout: Duration,
	map: HashMap<SocketAddr, (Timer, KillRequest, NonZeroU32)>,
	tx: Sender<KillRequest>,

	calc: TimeoutCalculator,
	state: State,
}

impl SuspicionTimers {
	pub(crate) fn new(
		base_timeout: Duration,
		calc: TimeoutCalculator,
		state: State,
	) -> (Receiver<KillRequest>, Self) {
		let (tx, rx) = channel(1);
		let this = Self {
			base_timeout,
			map: HashMap::new(),
			tx,
			calc,
			state,
		};
		(rx, this)
	}

	pub(crate) fn start(&mut self, kill_req: KillRequest) {
		let (min, max) = self.calc.min_max(&self.state);
		let d = self.calc.timeout(min, max, MIN_NON_ZERO_U32);

		let out = Output {
			value: kill_req,
			tx: self.tx.clone(),
		};

		let timer = Timer::new(d, out);

		self.map
			.insert(kill_req.addr, (timer, kill_req, MIN_NON_ZERO_U32));
	}

	pub(crate) fn remove(&mut self, addr: &SocketAddr) {
		self.map.remove(addr);
	}

	pub(super) fn update_node_count(&mut self, node_count: NonZeroU32) {
		self.state.node_count = node_count;

		self.reset_timers();
	}

	pub(super) fn update_ping_interval(&mut self, ping_interval: Duration) {
		self.state.ping_interval = ping_interval;

		self.reset_timers();
	}

	pub(crate) fn update_suspectors(&mut self, addr: &SocketAddr, suspectors: NonZeroUsize) {
		let suspectors = suspectors.try_into().unwrap_or(MAX_NON_ZERO_U32);

		if let Some((timer, kill_req, s)) = self.map.get_mut(addr) {
			*s = suspectors;

			let (min, max) = self.calc.min_max(&self.state);
			let d = self.calc.timeout(min, max, suspectors);

			let out = Output {
				value: *kill_req,
				tx: self.tx.clone(),
			};

			timer.reset(d, out);
		}
	}

	fn reset_timers(&mut self) {
		for (timer, kill_req, suspectors) in self.map.values_mut() {
			let (min, max) = self.calc.min_max(&self.state);
			let d = self.calc.timeout(min, max, *suspectors);

			let out = Output {
				value: *kill_req,
				tx: self.tx.clone(),
			};

			timer.reset(d, out);
		}
	}
}

#[cfg(test)]
mod tests {
	use std::convert::TryInto;

	use super::*;

	#[test]
	fn calc_timeout() {
		let t = TimeoutCalculator {
			alpha: 1.0,
			beta: 1.0,
			k: NonZeroU32::new(3).unwrap(), // <- only this is important
		};

		let cases = vec![
			(1, Duration::from_secs(30)),
			(2, Duration::from_secs(16)),
			(3, Duration::from_millis(7810)),
			(4, Duration::from_secs(2)),
			(5, Duration::from_secs(2)),
			(6, Duration::from_secs(2)),
		];

		for (c, expected) in cases {
			let result = t.timeout(
				Duration::from_secs(2),
				Duration::from_secs(30),
				c.try_into().unwrap(),
			);
			assert_eq!(result, expected);
		}
	}
}
