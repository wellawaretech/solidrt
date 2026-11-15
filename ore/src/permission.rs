//! Device permissions: the user's standing answer on the right to use a
//! device, as one of three states, and the request that asks for it. One
//! contract on every platform, the web's Permissions API states:
//!
//! - `Granted`: the app may use the device; an open is expected to stream.
//! - `Denied`: the user (or a policy) refused it; an open fails, and only
//!   the system settings change that.
//! - `Prompt`: the OS asks when the app requests or opens the device. It
//!   is also the honest state where the OS asks at the open and no probe
//!   reaches the answer ahead of it.
//!
//! The names are the web's (`camera`, `microphone`). Haptics and internet
//! are declarations (what the app uses), never permissions: no platform
//! asks the user for them.
//!
//! The mapping per platform, read by `state` (the first read probes, and
//! every return to the foreground probes again, `on_lifecycle`):
//! - Android: `checkSelfPermission` granted is `Granted`. Else
//!   `shouldShowRequestPermissionRationale` true is `Prompt` (denied once,
//!   a request still shows the dialog). Else the asked-before bit set is
//!   `Denied` (denied for good, or "don't ask again"). Else `Prompt` (never
//!   asked). Android answers "not granted, no rationale" both before the
//!   first request and after a final denial, so ore remembers that it asked:
//!   the bit per permission lives in the activity's SharedPreferences
//!   (`SolidRTActivity.permissionAsked`/`notePermissionAsked`), set before a
//!   request or a camera open, and cleared when the permission becomes
//!   granted. A read on return to the foreground catches an "only this time"
//!   grant that ended in the background and a change made in the settings.
//!   A refused request records this same mapping over fresh reads, not a
//!   flat `Denied`, so the answer and the resume that follows the dialog
//!   agree in either order: `Prompt` after a first denial (a request still
//!   shows the dialog), `Denied` only after "don't ask again" or the second
//!   denial. A granted request records `Granted`.
//! - macOS and iOS: `AVCaptureDevice.authorizationStatus(for:)` for video
//!   and audio. Not determined is `Prompt`, authorized `Granted`, denied and
//!   restricted `Denied`.
//! - Windows: the privacy switches under
//!   `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore`,
//!   `webcam` and `microphone`: the key's `Value` (apps may use the device)
//!   and its `NonPackaged\Value` (desktop apps may). Either one "Deny" is
//!   `Denied`; both "Allow" or absent is `Granted`. Windows has no prompt for
//!   a desktop app, so it never reads `Prompt`. The machine-wide switch an
//!   administrator sets (HKLM) is not read.
//! - Linux and the BSDs: no permission model outside a sandbox, so both
//!   read `Granted`. Inside Flatpak (`/.flatpak-info` exists) the camera
//!   portal decides at the open, so the camera reads `Prompt`; the
//!   microphone stays `Granted` (PulseAudio's socket, no portal).
//!
//! An answer corrects the state: a request's answer (`request`) and an open
//! camera's answer (`camera::Camera::permission`: approved is `Granted`,
//! denied is `Denied`) record it. The per-open session state (a camera's
//! `Pending | Approved | Denied`) is the progress of one open; this is the
//! standing fact a session reads before its open.
//!
//! A change reaches the pump as `PlatformEvent::Permission`, the way an
//! availability change does (`crate::availability`): `record` writes the
//! state, queues the change, then wakes the pump through the waker
//! `Platform::init` stored here. The first probe queues nothing (no reader
//! saw a state before it). With no pump (headless flux) nothing drains the
//! queue, and `state` still answers.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, Once, PoisonError};

use crate::lifecycle::Lifecycle;
use crate::platform::Waker;

/// A device permission the user answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Permission {
  /// Capturing from a camera.
  Camera,
  /// Recording from an audio input device.
  Microphone,
}

impl Permission {
  /// Every permission, in table order.
  pub const ALL: &[Permission] = &[Permission::Camera, Permission::Microphone];

  /// The permission's name, the web's: "camera" or "microphone".
  pub fn name(self) -> &'static str {
    match self {
      Permission::Camera => "camera",
      Permission::Microphone => "microphone",
    }
  }

  // The permission's slot in the table.
  fn index(self) -> usize {
    self as usize
  }
}

/// Where the user's answer on a permission stands (see the module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionState {
  /// The app may use the device.
  Granted,
  /// The app may not; only the system settings change that.
  Denied,
  /// The OS asks at the request or the open.
  Prompt,
}

impl PermissionState {
  /// Every state.
  pub const ALL: &[PermissionState] = &[PermissionState::Granted, PermissionState::Denied, PermissionState::Prompt];

  /// The state's name: "granted", "denied" or "prompt".
  pub fn name(self) -> &'static str {
    match self {
      PermissionState::Granted => "granted",
      PermissionState::Denied => "denied",
      PermissionState::Prompt => "prompt",
    }
  }

  fn to_u8(self) -> u8 {
    match self {
      PermissionState::Prompt => STATE_PROMPT,
      PermissionState::Granted => STATE_GRANTED,
      PermissionState::Denied => STATE_DENIED,
    }
  }

  fn from_u8(raw: u8) -> PermissionState {
    match raw {
      STATE_GRANTED => PermissionState::Granted,
      STATE_DENIED => PermissionState::Denied,
      _ => PermissionState::Prompt,
    }
  }
}

// The states as the table stores them.
const STATE_PROMPT: u8 = 0;
const STATE_GRANTED: u8 = 1;
const STATE_DENIED: u8 = 2;

// One state per permission, indexed by `Permission::index`; written by the
// first probe before any read.
static STATES: [AtomicU8; Permission::ALL.len()] = [const { AtomicU8::new(STATE_PROMPT) }; Permission::ALL.len()];

// The changes not yet drained by the pump, oldest first. Also the lock a
// write holds, so the queue's order is the order of the writes.
static CHANGES: Mutex<Vec<(Permission, PermissionState)>> = Mutex::new(Vec::new());

// The pump's waker, stored by `Platform::init`.
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

// The first probe of every permission, run once.
static PROBED: Once = Once::new();

/// The state of `permission` now (see the module doc). Any thread.
pub fn state(permission: Permission) -> PermissionState {
  probe_once();
  PermissionState::from_u8(STATES[permission.index()].load(Ordering::Acquire))
}

/// Record `state` for `permission`. On a change, queue it for the pump and
/// wake the pump; the state is written before the change is queued, so a
/// reader woken by the event reads the new state. A repeat of the current
/// state queues nothing. Any thread.
pub(crate) fn record(permission: Permission, state: PermissionState) {
  probe_once();
  let changed = {
    let mut changes = CHANGES.lock().unwrap_or_else(PoisonError::into_inner);
    let previous = STATES[permission.index()].swap(state.to_u8(), Ordering::AcqRel);
    let changed = previous != state.to_u8();
    if changed {
      changes.push((permission, state));
    }
    changed
  };
  if !changed {
    return;
  }
  // A granted permission is asked for afresh once it is revoked.
  #[cfg(target_os = "android")]
  if state == PermissionState::Granted {
    android::note_asked(permission, false);
  }
  if let Some(waker) = WAKER.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
    waker.wake();
  }
}

/// Store the pump's waker, so a change made while the pump sleeps wakes
/// it. Called once by `Platform::init`.
pub(crate) fn install_waker(waker: Waker) {
  *WAKER.lock().unwrap_or_else(PoisonError::into_inner) = Some(waker);
}

/// The changes queued since the last drain, oldest first. The pump's
/// watch poll and the tests.
#[cfg_attr(not(any(feature = "window", test)), allow(dead_code))]
pub(crate) fn drain() -> Vec<(Permission, PermissionState)> {
  std::mem::take(&mut *CHANGES.lock().unwrap_or_else(PoisonError::into_inner))
}

/// ore's lifecycle hook (`crate::platform::Platform::on_lifecycle`): back in
/// the foreground, every permission is read again and a change recorded (an
/// "only this time" grant ends in the background; the user may have
/// changed the settings meanwhile).
pub(crate) fn on_lifecycle(lifecycle: &Lifecycle) {
  if matches!(lifecycle, Lifecycle::DidForeground) {
    for permission in Permission::ALL {
      if let Some(state) = probe(*permission) {
        record(*permission, state);
      }
    }
  }
}

/// Note that the OS is about to ask for `permission` on a path ore does not
/// request through (SDL's own camera open asks Android for the camera), so
/// a later "not granted, no rationale" reads `Denied`. A no-op where the
/// permission is granted, and on every platform but Android.
pub(crate) fn note_asking(permission: Permission) {
  #[cfg(target_os = "android")]
  if state(permission) != PermissionState::Granted {
    android::note_asked(permission, true);
  }
  #[cfg(not(target_os = "android"))]
  let _ = permission;
}

/// Ask for `permission` without waiting for the answer: `on_answer` runs
/// once with the state, on a thread of the platform's choice (Android: the
/// activity's thread), or before this returns where no request is raised.
///
/// Android raises its runtime prompt (answering at once when already
/// granted) and answers this request's outcome, `Granted` or `Denied`; the
/// standing state it records is `answered_state` (a refusal is the
/// mapping's answer, `Prompt` after a first denial), so a caller that
/// opens on `Prompt` elsewhere never opens on a refusal here. Every other
/// platform has no prompt an app raises ahead of use: there the
/// answer is `state(permission)` at once, and `Prompt` means the OS asks at
/// the open, whose result the open reports. SDL's own audio recording open
/// asks Android for the microphone with a blocking callback (SDL_aaudio.c
/// spins until the answer), so a caller asks here first and opens once
/// granted, when SDL's own request answers at once. The error is the
/// platform's text when the request could not be made; `on_answer` is
/// dropped unrun then.
pub fn request(permission: Permission, on_answer: impl FnOnce(PermissionState) + Send + 'static) -> Result<(), String> {
  #[cfg(target_os = "android")]
  {
    note_asking(permission);
    android::request(permission, Box::new(on_answer))
  }
  #[cfg(not(target_os = "android"))]
  {
    on_answer(state(permission));
    Ok(())
  }
}

// The first answer of every permission, stored without a change event: no
// reader sees a state before this ran (`state` and `record` run it first).
// A probe that cannot answer (no Android activity yet) leaves `Prompt`.
fn probe_once() {
  PROBED.call_once(|| {
    for permission in Permission::ALL {
      let state = probe(*permission).unwrap_or(PermissionState::Prompt);
      STATES[permission.index()].store(state.to_u8(), Ordering::Release);
    }
  });
}

/// The platform's answer for `permission` now (see the module doc); None
/// when the platform could not be asked.
fn probe(permission: Permission) -> Option<PermissionState> {
  #[cfg(target_os = "android")]
  {
    android::probe(permission)
  }
  #[cfg(any(target_os = "macos", target_os = "ios"))]
  {
    Some(apple::probe(permission))
  }
  #[cfg(target_os = "windows")]
  {
    Some(consent_store::probe(permission))
  }
  #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android", target_os = "ios")))]
  {
    const FLATPAK_INFO: &str = "/.flatpak-info";
    Some(unix_state(permission, std::path::Path::new(FLATPAK_INFO).exists()))
  }
}

/// The standing state an Android request's answer records (see the module
/// doc): `Granted` when granted; on a refusal, `probed`, the mapping over
/// fresh reads, or `Denied` when the activity could not be read.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
pub(crate) fn answered_state(granted: bool, probed: impl FnOnce() -> Option<PermissionState>) -> PermissionState {
  if granted {
    PermissionState::Granted
  } else {
    probed().unwrap_or(PermissionState::Denied)
  }
}

/// The Android mapping (see the module doc): `granted` is
/// `checkSelfPermission`'s answer, `rationale`
/// `shouldShowRequestPermissionRationale`'s, `asked` the asked-before bit.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
pub(crate) fn android_state(granted: bool, rationale: bool, asked: bool) -> PermissionState {
  if granted {
    PermissionState::Granted
  } else if rationale {
    PermissionState::Prompt
  } else if asked {
    PermissionState::Denied
  } else {
    PermissionState::Prompt
  }
}

/// The Windows mapping (see the module doc): `consent` is the capability
/// key's `Value`, `non_packaged` its `NonPackaged\Value`, None where absent.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) fn windows_state(consent: Option<&str>, non_packaged: Option<&str>) -> PermissionState {
  const DENY: &str = "Deny";
  if consent == Some(DENY) || non_packaged == Some(DENY) {
    PermissionState::Denied
  } else {
    PermissionState::Granted
  }
}

/// The Linux and BSD mapping (see the module doc): `flatpak` is whether
/// the process runs inside a Flatpak sandbox.
#[cfg_attr(any(target_os = "windows", target_os = "macos", target_os = "android", target_os = "ios"), allow(dead_code))]
pub(crate) fn unix_state(permission: Permission, flatpak: bool) -> PermissionState {
  match permission {
    Permission::Camera if flatpak => PermissionState::Prompt,
    _ => PermissionState::Granted,
  }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
  use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio, AVMediaTypeVideo};

  use super::{Permission, PermissionState};

  pub(super) fn probe(permission: Permission) -> PermissionState {
    // SAFETY: the media type constants are immutable statics AVFoundation
    // exports, read once here.
    let media = unsafe {
      match permission {
        Permission::Camera => AVMediaTypeVideo,
        Permission::Microphone => AVMediaTypeAudio,
      }
    };
    let Some(media) = media else {
      log::warn!("[ore] AVFoundation has no media type for the {}; reporting it prompt", permission.name());
      return PermissionState::Prompt;
    };
    // SAFETY: a class method callable from any thread; it throws only for
    // a media type other than video and audio, and these are the two.
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media) };
    if status == AVAuthorizationStatus::Authorized {
      PermissionState::Granted
    } else if status == AVAuthorizationStatus::Denied || status == AVAuthorizationStatus::Restricted {
      PermissionState::Denied
    } else {
      PermissionState::Prompt
    }
  }
}

#[cfg(target_os = "windows")]
mod consent_store {
  use windows_registry::CURRENT_USER;

  use super::{windows_state, Permission, PermissionState};

  // The per-user privacy switches, one subkey per capability.
  const CONSENT_STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
  // The desktop-app switch, a subkey of the capability's key.
  const NON_PACKAGED: &str = "NonPackaged";
  // The switch's value name.
  const VALUE: &str = "Value";

  pub(super) fn probe(permission: Permission) -> PermissionState {
    let capability = match permission {
      Permission::Camera => "webcam",
      Permission::Microphone => "microphone",
    };
    let key = CURRENT_USER.open(format!(r"{CONSENT_STORE}\{capability}")).ok();
    let consent = key.as_ref().and_then(|key| key.get_string(VALUE).ok());
    let non_packaged =
      key.as_ref().and_then(|key| key.open(NON_PACKAGED).ok()).and_then(|key| key.get_string(VALUE).ok());
    windows_state(consent.as_deref(), non_packaged.as_deref())
  }
}

#[cfg(target_os = "android")]
mod android {
  use std::ffi::{c_char, c_void, CStr};
  use std::panic::AssertUnwindSafe;

  use jni::objects::JValue;
  use sdl3::sys::system::SDL_RequestAndroidPermission;

  use super::{android_state, answered_state, record, Permission, PermissionState};

  type Answer = Box<dyn FnOnce(PermissionState) + Send>;

  // PackageManager.PERMISSION_GRANTED, what checkSelfPermission answers
  // for a held permission.
  const PERMISSION_GRANTED: i32 = 0;

  /// The Android name of a permission.
  fn name(permission: Permission) -> &'static CStr {
    match permission {
      Permission::Camera => c"android.permission.CAMERA",
      Permission::Microphone => c"android.permission.RECORD_AUDIO",
    }
  }

  fn name_str(permission: Permission) -> &'static str {
    name(permission).to_str().expect("an Android permission name is ASCII")
  }

  /// The mapping's three facts, read from the activity; None (warned) when
  /// the activity cannot be asked.
  pub(super) fn probe(permission: Permission) -> Option<PermissionState> {
    let facts = crate::android::with_activity("permission state", |env, activity| {
      let jname = env.new_string(name_str(permission))?;
      let granted = env
        .call_method(
          activity,
          jni::jni_str!("checkSelfPermission"),
          jni::jni_sig!("(Ljava/lang/String;)I"),
          &[JValue::Object(&jname)],
        )?
        .i()?
        == PERMISSION_GRANTED;
      let rationale = env
        .call_method(
          activity,
          jni::jni_str!("shouldShowRequestPermissionRationale"),
          jni::jni_sig!("(Ljava/lang/String;)Z"),
          &[JValue::Object(&jname)],
        )?
        .z()?;
      let class = env.get_object_class(activity)?;
      let asked = env
        .call_static_method(
          &class,
          jni::jni_str!("permissionAsked"),
          jni::jni_sig!("(Ljava/lang/String;)Z"),
          &[JValue::Object(&jname)],
        )?
        .z()?;
      Ok((granted, rationale, asked))
    });
    match facts {
      Ok((granted, rationale, asked)) => {
        if granted && asked {
          note_asked(permission, false);
        }
        Some(android_state(granted, rationale, asked))
      }
      Err(e) => {
        log::warn!("[ore] {e}; keeping the {} permission state", permission.name());
        None
      }
    }
  }

  /// Set or clear the asked-before bit of `permission`.
  pub(super) fn note_asked(permission: Permission, asked: bool) {
    let noted = crate::android::with_activity("permission note", |env, activity| {
      let jname = env.new_string(name_str(permission))?;
      let class = env.get_object_class(activity)?;
      env
        .call_static_method(
          &class,
          jni::jni_str!("notePermissionAsked"),
          jni::jni_sig!("(Ljava/lang/String;Z)V"),
          &[JValue::Object(&jname), JValue::Bool(asked)],
        )?
        .v()
    });
    if let Err(e) = noted {
      log::warn!("[ore] {e}; the {} permission's asked-before bit is not stored", permission.name());
    }
  }

  pub(super) fn request(permission: Permission, on_answer: Answer) -> Result<(), String> {
    let lent = Box::into_raw(Box::new((permission, on_answer)));
    // SAFETY: the name is a static C string; `lent` is an owned allocation
    // that `answered` takes back exactly once, or this function does below
    // when SDL refuses the request and never calls it.
    if unsafe { SDL_RequestAndroidPermission(name(permission).as_ptr(), Some(answered), lent as *mut c_void) } {
      return Ok(());
    }
    // SAFETY: SDL did not take the request, so `lent` is still ours alone.
    drop(unsafe { Box::from_raw(lent) });
    Err(crate::sdl_error())
  }

  /// SDL's answer to a request: runs once, on the activity's thread. The
  /// standing state (`answered_state`) is recorded before `on_answer` runs
  /// with the request's outcome. A panic must not cross the C boundary (it
  /// would abort), so it ends here.
  unsafe extern "C" fn answered(userdata: *mut c_void, _permission: *const c_char, granted: bool) {
    // SAFETY: `userdata` is the allocation `request` lent, handed back once.
    let lent = unsafe { Box::from_raw(userdata as *mut (Permission, Answer)) };
    let (permission, on_answer) = *lent;
    let outcome = if granted { PermissionState::Granted } else { PermissionState::Denied };
    if std::panic::catch_unwind(AssertUnwindSafe(move || {
      record(permission, answered_state(granted, || probe(permission)));
      on_answer(outcome);
    }))
    .is_err()
    {
      log::error!("[ore] A permission answer panicked");
    }
  }
}
