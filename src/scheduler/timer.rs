use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use tokio::sync::mpsc::Sender;
use tokio::time::sleep;

use crate::handle::Handle;

#[derive(Debug, Clone)]
pub(crate) struct Output<T: Send> {
	pub(crate) value: T,
	pub(crate) tx: Sender<T>,
}

impl<T: Send> Output<T> {
	#[allow(unused_must_use)] // ignore the unused Result from `send`, since we don't care if the send succeeds.
	async fn send(self) {
		self.tx.send(self.value).await;
	}
}

#[derive(Debug)]
pub(super) struct Timer {
	started: Instant,
	handle: Handle,
	done: Arc<AtomicBool>, // check so we don't accidentally restart the timer again during a reset.
}

impl Timer {
	pub(super) fn new<T>(d: Duration, out: Output<T>) -> Self
	where
		T: 'static + Send,
	{
		let done = Arc::new(AtomicBool::new(false));

		let handle = tokio::spawn(task(d, done.clone(), out));

		Self {
			started: Instant::now(),
			handle: Handle::from(handle),
			done,
		}
	}

	pub(super) fn reset<T>(&mut self, d: Duration, out: Output<T>)
	where
		T: 'static + Send,
	{
		let is_done = self.done.swap(true, Ordering::Acquire);
		if is_done {
			return;
		}

		self.handle.inner.abort();

		self.done.store(false, Ordering::Release);

		let now = Instant::now();
		let passed = now - self.started;

		// TODO: replace with `Duration::saturating_sub` once stable
		let d = match d.checked_sub(passed) {
			Some(d) => d,
			None => Duration::from_nanos(0),
		};

		let handle = tokio::spawn(task(d, self.done.clone(), out));

		self.handle = Handle::from(handle);
	}
}

async fn task<T>(d: Duration, done: Arc<AtomicBool>, out: Output<T>)
where
	T: 'static + Send,
{
	sleep(d).await;

	let is_done = done.swap(true, Ordering::AcqRel);

	if !is_done {
		out.send().await;
	}
}

#[cfg(test)]
mod tests {
	use tokio::sync::mpsc::channel;

	use super::*;

	#[tokio::test]
	async fn task_sends_output_if_done_is_false() {
		let (tx, mut rx) = channel(1);
		let out = Output { value: (), tx };
		let done = Arc::new(AtomicBool::new(false));

		task(Duration::from_nanos(0), done.clone(), out).await;

		assert!(done.load(Ordering::Relaxed));
		assert!(rx.recv().await.is_some());
	}

	#[tokio::test]
	async fn task_does_not_send_output_if_done_is_true() {
		let (tx, mut rx) = channel(1);
		let out = Output { value: (), tx };
		let done = Arc::new(AtomicBool::new(true));

		task(Duration::from_nanos(0), done.clone(), out).await;

		assert!(done.load(Ordering::Relaxed));
		assert!(rx.recv().await.is_none());
	}
}
