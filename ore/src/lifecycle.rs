//! The app lifecycle: what the platform grants an app on its way to the
//! background. The lifecycle events themselves come with the event pump.

/// Held while the app's suspend handlers run: the platform's permission to
/// keep executing after it has asked the app to background. Taken at push
/// time of WILL_ENTER_BACKGROUND (the last moment the platform still
/// listens), carried with the suspend event, and released when the last
/// clone drops - the embedder drops it once the handlers finished or their
/// deadline passed. iOS is the platform that needs it: a UIApplication
/// background task assertion begun in the delegate callback keeps every
/// thread running for the system's budget (about 30 s) instead of freezing
/// the process moments after the callback returns; that assertion is this
/// guard's implementation when the iOS port lands. Android does not freeze
/// a backgrounded task's process, so the hold has nothing to do there, and
/// desktop never suspends.
#[derive(Clone)]
pub struct SuspendHold {
  _platform: std::sync::Arc<SuspendHoldInner>,
}

struct SuspendHoldInner;

impl SuspendHold {
  pub fn take() -> Self {
    Self { _platform: std::sync::Arc::new(SuspendHoldInner) }
  }
}

impl Drop for SuspendHoldInner {
  fn drop(&mut self) {
    // No platform assertion to end yet (see SuspendHold).
  }
}
