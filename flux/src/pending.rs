use rquickjs::JsLifetime;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Tracks pending async operations that should keep the engine alive.
#[derive(Clone, JsLifetime)]
pub(crate) struct PendingOps {
  #[qjs(skip_trace)]
  inner: Arc<PendingOpsInner>,
}

struct PendingOpsInner {
  count: AtomicU32,
  notify: tokio::sync::Notify,
}

impl PendingOps {
  pub(crate) fn new() -> Self {
    Self { inner: Arc::new(PendingOpsInner { count: AtomicU32::new(0), notify: tokio::sync::Notify::new() }) }
  }

  pub(crate) fn hold(&self) {
    self.inner.count.fetch_add(1, Ordering::SeqCst);
  }

  pub(crate) fn release(&self) {
    let prev = self.inner.count.fetch_sub(1, Ordering::SeqCst);
    if prev == 1 {
      self.inner.notify.notify_waiters();
    }
  }

  pub(crate) fn is_idle(&self) -> bool {
    self.inner.count.load(Ordering::SeqCst) == 0
  }

  /// The release-to-zero notification as a future, not awaited here: the
  /// engine loop pins it and calls `enable()` to register interest before
  /// re-checking the count (Notify stores no permit, so registering after a
  /// racing release would lose the wakeup).
  pub(crate) fn notified(&self) -> tokio::sync::futures::Notified<'_> {
    self.inner.notify.notified()
  }
}
