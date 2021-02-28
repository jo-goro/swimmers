use tokio::task::JoinHandle;

/// Wrapper around a [JoinHandle] which aborts the handle when dropped.
#[derive(Debug)]
pub(crate) struct Handle {
	inner: JoinHandle<()>,
}

impl Handle {
	#[inline]
	pub(crate) fn abort(&self) {
		self.inner.abort();
	}
}

impl From<JoinHandle<()>> for Handle {
	fn from(handle: JoinHandle<()>) -> Self {
		Self { inner: handle }
	}
}

impl Drop for Handle {
	fn drop(&mut self) {
		self.inner.abort();
	}
}
