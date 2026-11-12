//! The app lifecycle: the background and foreground transitions as the
//! platform pushes them (`Platform::on_lifecycle`), and what the platform
//! grants an app on its way to the background (`SuspendHold`).

/// A lifecycle transition, delivered synchronously on the thread pushing
/// SDL's app event (`crate::platform::Platform::on_lifecycle`), not through
/// the pump's queue.
///
/// Timely background delivery on Android: with BLOCK_ON_PAUSE the pump
/// blocks before the queued background events are drained, so through the
/// normal path the app would learn about backgrounding only at resume
/// (device-observed). An event watch runs synchronously on the thread
/// pushing the event - for the background events that is the main thread,
/// inside the blocking wait - so each transition is forwarded the moment
/// it is queued. DID_ENTER_BACKGROUND's queued copy still arrives at
/// resume, through the pump as `PlatformEvent::Visibility`; consumers
/// tolerate the repeat.
///
/// WILL_ENTER_BACKGROUND is the suspend signal on both mobile platforms
/// (SDL's Android_OnPause sends it right before the DID event, and its
/// comment names an event filter at queue time as the place for lifecycle
/// handling), and the watch is its only source: Android_OnDestroy flushes
/// the queue before sending Quit, so a queued copy never reaches a
/// finishing app, and translating the copy would replay the suspend at
/// resume (the pump does not translate it). The hold is taken at push time,
/// inside the platform callback, which is the last moment the platform
/// still listens (see SuspendHold).
pub enum Lifecycle {
  /// WILL_ENTER_BACKGROUND: the app is being suspended and may be killed
  /// without further notice. Carries the hold that keeps the platform
  /// running while the app's suspend handlers execute. Never fires on
  /// desktop: a minimized desktop app is not killed.
  WillBackground(SuspendHold),
  /// DID_ENTER_BACKGROUND.
  DidBackground,
  /// DID_ENTER_FOREGROUND: on Android, SDL has just restarted its audio
  /// devices (Android_OnResume runs the restart right before it sends this
  /// event).
  DidForeground,
}

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
