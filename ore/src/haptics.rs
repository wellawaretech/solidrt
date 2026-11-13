//! Haptic feedback: the device's own actuator answering what just
//! happened in the app (a switch that flipped, a rejected input, a hit).
//! The app names the intent, not the feel: each platform plays its native
//! effect for it, follows the user's system settings (the touch feedback
//! setting always wins), and plays a nearest effect or nothing where it
//! has none. No platform constant appears here.
//!
//! Android only, for now: the activity's static helpers
//! (`SolidRTActivity.haptic`, `hapticSupport`, `hapticStop`, reached over
//! JNI on the JavaVM and activity alloy publishes in ndk-context; no SDL,
//! whose haptic API plays one-shots only) map an intent by SDK level to
//! `View.performHapticFeedback`, which needs no permission, or to the
//! `Vibrator` with usage TOUCH, which needs VIBRATE (the haptics
//! capability). Every other platform has no actuator here: support is
//! `Support::none()` and every call a no-op. iOS, macOS and Windows are
//! backends added later behind the same intents.
//!
//! Every call is accepted everywhere; where nothing plays it is a silent
//! no-op, except one warning per process when an intent on Android's
//! `Vibrator` path meets an app without VIBRATE. `Support` is a fact of
//! the hardware and the build (the actuator, the level of each intent,
//! whether `impact` intensity is honoured), probed once (`available`,
//! `Session::support`); an intent the user silenced is still `native`.
//! Going to the background cancels a playing effect (`on_lifecycle`).
//!
//! `Session` is the session an embedder keeps per engine, the shape of
//! `link::Session`: every call is logged with its time, and a session set
//! to record only plays nothing and reports a device that supports every
//! intent natively, as a test host's engine does.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use crate::lifecycle::Lifecycle;

/// How many calls the log keeps, the newest last: what a test or an agent
/// reads back, without an app that plays feedback every frame growing it.
pub(crate) const REQUESTED_LOG_CAP: usize = 64;

/// The intensity an impact plays at when the app names none: full.
const DEFAULT_INTENSITY: f32 = 1.0;

/// Milliseconds in a second, for the log's times.
const MS_PER_SECOND: f64 = 1000.0;

/// What happened in the app, which the platform answers with its effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
  /// Moved to another discrete choice (a picker, a slider step).
  Selection,
  /// Snapped to a guide or an edge.
  Alignment,
  /// A drag crossed the point where release acts (pull to refresh armed).
  ThresholdOn,
  /// The drag crossed back.
  ThresholdOff,
  /// A switch turned on.
  ToggleOn,
  /// A switch turned off.
  ToggleOff,
  /// A long press triggered an action.
  LongPress,
  /// An item was picked up.
  DragStart,
  /// An action finished.
  Success,
  /// An action failed, an input was rejected.
  Error,
  /// An action needs care.
  Warning,
  /// A collision, a landing, a physical hit; the one intent with a weight
  /// and an intensity (`Impact`).
  Impact,
}

impl Intent {
  /// Every intent, in wire order (`index`).
  pub const ALL: [Intent; 12] = [
    Intent::Selection,
    Intent::Alignment,
    Intent::ThresholdOn,
    Intent::ThresholdOff,
    Intent::ToggleOn,
    Intent::ToggleOff,
    Intent::LongPress,
    Intent::DragStart,
    Intent::Success,
    Intent::Error,
    Intent::Warning,
    Intent::Impact,
  ];

  /// The intent's name in the app-facing API (`"thresholdOn"`).
  pub fn name(self) -> &'static str {
    match self {
      Intent::Selection => "selection",
      Intent::Alignment => "alignment",
      Intent::ThresholdOn => "thresholdOn",
      Intent::ThresholdOff => "thresholdOff",
      Intent::ToggleOn => "toggleOn",
      Intent::ToggleOff => "toggleOff",
      Intent::LongPress => "longPress",
      Intent::DragStart => "dragStart",
      Intent::Success => "success",
      Intent::Error => "error",
      Intent::Warning => "warning",
      Intent::Impact => "impact",
    }
  }

  /// The intent `name` names, if any.
  pub fn from_name(name: &str) -> Option<Intent> {
    Intent::ALL.into_iter().find(|intent| intent.name() == name)
  }

  /// The intent's place in `ALL`: its number on the wire and in
  /// `Support::levels`.
  pub fn index(self) -> usize {
    self as usize
  }
}

/// How heavy an impact is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weight {
  Light,
  #[default]
  Medium,
  Heavy,
}

impl Weight {
  /// The weight's name in the app-facing API (`"heavy"`).
  pub fn name(self) -> &'static str {
    match self {
      Weight::Light => "light",
      Weight::Medium => "medium",
      Weight::Heavy => "heavy",
    }
  }

  /// The weight `name` names, if any.
  pub fn from_name(name: &str) -> Option<Weight> {
    [Weight::Light, Weight::Medium, Weight::Heavy].into_iter().find(|weight| weight.name() == name)
  }

  /// The weight's number on the wire: light 0, medium 1, heavy 2.
  pub fn index(self) -> usize {
    self as usize
  }
}

/// What an `Intent::Impact` carries: its weight and its intensity, 0..1
/// (where the platform honours it, `Support::impact_intensity`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impact {
  pub weight: Weight,
  pub intensity: f32,
}

impl Default for Impact {
  fn default() -> Impact {
    Impact { weight: Weight::default(), intensity: DEFAULT_INTENSITY }
  }
}

/// One feedback call: the intent, and for `Intent::Impact` (only) its
/// impact. `Session::feedback` normalises a call so that `impact` is Some
/// exactly for `Intent::Impact`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Call {
  pub intent: Intent,
  pub impact: Option<Impact>,
}

/// How an intent plays on this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
  /// The platform has its own effect for the intent.
  Native,
  /// Another intent's effect plays for it.
  Approximate,
  /// Nothing plays.
  None,
}

impl Level {
  /// The level's name in the app-facing API.
  pub fn name(self) -> &'static str {
    match self {
      Level::Native => "native",
      Level::Approximate => "approximate",
      Level::None => "none",
    }
  }
}

/// What this device plays: hardware and build facts, not the user's
/// settings (see the module doc).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Support {
  /// Whether the device has a haptic actuator at all.
  pub actuator: bool,
  /// Each intent's level, by `Intent::index`.
  pub levels: [Level; 12],
  /// Whether an impact's intensity changes what plays.
  pub impact_intensity: bool,
}

impl Support {
  /// How `intent` plays.
  pub fn level(&self, intent: Intent) -> Level {
    self.levels[intent.index()]
  }

  /// What a record-only session reports: a device that plays every intent
  /// natively, intensity included.
  pub fn record_only() -> Support {
    Support { actuator: true, levels: [Level::Native; 12], impact_intensity: true }
  }

  /// A device without an actuator: nothing plays.
  pub fn none() -> Support {
    Support { actuator: false, levels: [Level::None; 12], impact_intensity: false }
  }
}

/// One logged call and when it came, in ms since the session was created.
#[derive(Clone, Debug, PartialEq)]
pub struct Recorded {
  pub call: Call,
  pub at_ms: f64,
}

/// Whether this device has a haptic actuator (the platform's
/// `Support::actuator`, probed once). An embedder lists the capability
/// from it.
pub fn available() -> bool {
  platform_support().actuator
}

/// The platform's support, probed once for the process.
fn platform_support() -> Support {
  static SUPPORT: OnceLock<Support> = OnceLock::new();
  *SUPPORT.get_or_init(probe_support)
}

#[cfg(target_os = "android")]
fn probe_support() -> Support {
  android::support()
}

#[cfg(not(target_os = "android"))]
fn probe_support() -> Support {
  Support::none()
}

/// ore's own handling of the lifecycle watch
/// (`crate::platform::Platform::on_lifecycle`): going to the background
/// cancels a playing effect (Android).
pub(crate) fn on_lifecycle(lifecycle: &Lifecycle) {
  if matches!(lifecycle, Lifecycle::DidBackground) {
    cancel();
  }
}

/// Play `call` on the platform now.
#[cfg(target_os = "android")]
fn play(call: Call) {
  android::play(call);
}

#[cfg(not(target_os = "android"))]
fn play(_call: Call) {}

/// Cancel what plays on the platform now.
fn cancel() {
  #[cfg(target_os = "android")]
  android::stop();
}

/// `call` as the log keeps it and the platform plays it: an impact always
/// has its weight and intensity, every other intent none.
fn normalise(call: Call) -> Call {
  let impact = match call.intent {
    Intent::Impact => Some(call.impact.unwrap_or_default()),
    _ => None,
  };
  Call { intent: call.intent, impact }
}

/// The haptics session of one engine (see the module doc).
pub struct Session {
  record_only: AtomicBool,
  started: Instant,
  requested: Mutex<VecDeque<Recorded>>,
}

impl Session {
  pub fn new() -> Arc<Session> {
    Arc::new(Session {
      record_only: AtomicBool::new(false),
      started: Instant::now(),
      requested: Mutex::new(VecDeque::new()),
    })
  }

  /// Never reach the platform in this session: every call is logged and
  /// plays nothing, and support reports `Support::record_only()` (a test
  /// host's engine, a render).
  pub fn set_record_only(&self) {
    self.record_only.store(true, Ordering::Relaxed);
  }

  pub fn is_record_only(&self) -> bool {
    self.record_only.load(Ordering::Relaxed)
  }

  /// What this session's device plays: `Support::record_only()` in a
  /// record-only session, the platform's otherwise.
  pub fn support(&self) -> Support {
    if self.is_record_only() {
      Support::record_only()
    } else {
      platform_support()
    }
  }

  /// Play `call`, logging it first (normalised, see `Call`); a record-only
  /// session plays nothing.
  pub fn feedback(&self, call: Call) {
    let call = normalise(call);
    {
      let at_ms = self.started.elapsed().as_secs_f64() * MS_PER_SECOND;
      let mut requested = self.requested.lock().unwrap_or_else(PoisonError::into_inner);
      if requested.len() == REQUESTED_LOG_CAP {
        requested.pop_front();
      }
      requested.push_back(Recorded { call, at_ms });
    }
    if !self.is_record_only() {
      play(call);
    }
  }

  /// Cancel what plays on the platform now; a record-only session does
  /// nothing.
  pub fn stop(&self) {
    if self.is_record_only() {
      return;
    }
    cancel();
  }

  /// The calls the app made in this session, oldest first, the last
  /// `REQUESTED_LOG_CAP` of them, whatever played.
  pub fn requested(&self) -> Vec<Recorded> {
    self.requested.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
  }
}

#[cfg(target_os = "android")]
mod android {
  use super::{Call, Intent, Level, Support};
  use jni::objects::{JIntArray, JValue};
  use std::sync::atomic::{AtomicBool, Ordering};

  // What `SolidRTActivity.haptic` returns.
  const PLAYED: i32 = 0;
  const NO_ACTUATOR: i32 = 1;
  const NO_PERMISSION: i32 = 2;
  const UNSUPPORTED: i32 = 3;

  // An intent's level in what `SolidRTActivity.hapticSupport` returns.
  const LEVEL_NATIVE: i32 = 0;
  const LEVEL_APPROXIMATE: i32 = 1;

  // The slots of `SolidRTActivity.hapticSupport`'s array: three flags
  // (1 true, 0 false), then one level per intent in `Intent::ALL` order.
  const ACTUATOR_SLOT: usize = 0;
  const IMPACT_INTENSITY_SLOT: usize = 1;
  const VIBRATE_PERMITTED_SLOT: usize = 2;
  const FIRST_LEVEL_SLOT: usize = 3;
  const SUPPORT_LEN: usize = FIRST_LEVEL_SLOT + Intent::ALL.len();

  // One warning per process for an app without VIBRATE, and one for a
  // failing JNI call: an app may play feedback on every slider step.
  static NO_PERMISSION_WARNED: AtomicBool = AtomicBool::new(false);
  static FAILURE_WARNED: AtomicBool = AtomicBool::new(false);

  pub(super) fn support() -> Support {
    let queried = crate::android::with_activity("haptic support query", |env, activity| {
      let class = env.get_object_class(activity)?;
      let facts = env.call_static_method(&class, jni::jni_str!("hapticSupport"), jni::jni_sig!("()[I"), &[])?.l()?;
      let facts = env.cast_local::<JIntArray>(facts)?;
      let mut out = [0; SUPPORT_LEN];
      facts.get_region(env, 0, &mut out)?;
      Ok(out)
    });
    let facts = match queried {
      Ok(facts) => facts,
      Err(e) => {
        log::warn!("[ore] {e}; reporting no haptic actuator");
        return Support::none();
      }
    };
    let mut levels = [Level::None; Intent::ALL.len()];
    for (level, fact) in levels.iter_mut().zip(&facts[FIRST_LEVEL_SLOT..]) {
      *level = match *fact {
        LEVEL_NATIVE => Level::Native,
        LEVEL_APPROXIMATE => Level::Approximate,
        _ => Level::None,
      };
    }
    let support =
      Support { actuator: facts[ACTUATOR_SLOT] != 0, levels, impact_intensity: facts[IMPACT_INTENSITY_SLOT] != 0 };
    log::debug!("[ore] Haptic support: {support:?}, VIBRATE permitted: {}", facts[VIBRATE_PERMITTED_SLOT] != 0);
    support
  }

  pub(super) fn play(call: Call) {
    let impact = call.impact.unwrap_or_default();
    let played = crate::android::with_activity("haptic feedback", |env, activity| {
      let class = env.get_object_class(activity)?;
      env
        .call_static_method(
          &class,
          jni::jni_str!("haptic"),
          jni::jni_sig!("(IIF)I"),
          &[
            JValue::Int(call.intent.index() as i32),
            JValue::Int(impact.weight.index() as i32),
            JValue::Float(impact.intensity),
          ],
        )?
        .i()
    });
    match played {
      Ok(PLAYED | NO_ACTUATOR | UNSUPPORTED) => {}
      Ok(NO_PERMISSION) => {
        if !NO_PERMISSION_WARNED.swap(true, Ordering::Relaxed) {
          log::warn!(
            "[ore] Haptic feedback \"{}\" plays nothing: the app has no VIBRATE permission; switch the haptics capability on in package.json (solidrt.capabilities)",
            call.intent.name()
          );
        }
      }
      Ok(other) => warn_failure(&format!("Haptic feedback answered an unknown result {other}")),
      Err(e) => warn_failure(&e),
    }
  }

  pub(super) fn stop() {
    let stopped = crate::android::with_activity("haptic stop", |env, activity| {
      let class = env.get_object_class(activity)?;
      env.call_static_method(&class, jni::jni_str!("hapticStop"), jni::jni_sig!("()V"), &[])?.v()
    });
    if let Err(e) = stopped {
      warn_failure(&e);
    }
  }

  fn warn_failure(error: &str) {
    if !FAILURE_WARNED.swap(true, Ordering::Relaxed) {
      log::warn!("[ore] {error}; later haptic failures are not logged");
    }
  }
}
