//! System file dialogs: the platform's own open, save and folder pickers,
//! and the vocabulary they are asked and answered in. Part of ore's window
//! part (the `window` feature): a dialog shows with the app window as its
//! parent, and its answer arrives through the event pump.
//!
//! A dialog is shown over the main-thread bridge (`main_thread::run`; SDL's
//! dialog calls are main thread only), with the live window as its parent
//! (`window::dialog_parent`), and answered through a responder once the
//! user is done, from whatever thread the platform answers on. Desktop goes
//! through SDL (the xdg-desktop-portal file chooser with a zenity fallback
//! on Linux, IFileDialog on Windows, NSOpenPanel as a sheet on the app
//! window on macOS). Android goes through the activity's own document
//! intents (`SolidRTActivity.showDialog`): SDL's Android backend takes no
//! persistable grant on what was picked and has no folder picker, so the
//! activity owns the intents and reports the result over JNI
//! (`android_result_raw`). What comes back is a location string a file
//! handle opens later: a path on desktop, a `content://` URI on Android. A
//! cancel is an empty answer, never an error.
//!
//! Availability (`availability`, see `crate::availability`) is probed once
//! the window exists, and reads Unknown before that. Unavailable on a
//! hidden or offscreen surface, on a platform without a backend (iOS), on
//! a Linux session with neither the portal nor zenity, and on an Android
//! whose document picker is missing or is the TV framework's stub (which
//! cancels every request). Available on Windows and macOS, on a Linux
//! session with zenity on the path (SDL's fallback), and on an Android
//! with a real document picker. Unknown on a Linux session with the portal
//! alone, until the portal introspection (`crate::portal`, started by this
//! probe or the link's) settles it on the portal's FileChooser interface;
//! where the introspection fails, it stays Unknown until the first call.
//! A call corrects the fact before it answers: SDL's "driver unsupported"
//! (`DialogError::Unavailable`) records Unavailable, a platform answer that
//! worked (a cancel included) on Unknown records Available. A request is
//! refused at once only where dialogs are Unavailable.
//!
//! `Session` is the session an embedder keeps per engine: one dialog at a
//! time (the platform's are modal), the request waiting for its answer, the
//! last request made with its answer, and the answers a test or an agent
//! gives in the user's place. An answer given ahead is what the next
//! request takes instead of showing a dialog; an answer arriving while a
//! request waits settles it at once, and a native dialog still open then
//! has its own answer dropped (SDL cannot dismiss one it showed); a session
//! set to answers only never shows anything but waits on the queue, as a
//! test host's engine does. Like a test microphone or a still camera, the
//! session is where a test takes the platform's place.

#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod desktop;
mod session;

#[cfg(target_os = "android")]
use android as platform;
#[cfg(not(target_os = "android"))]
use desktop as platform;

use crate::availability::{self, Availability, Facility};

/// The activity's answer to a dialog, from the embedder's exported JNI
/// symbol (`Java_com_solidrt_app_SolidRTActivity_nativeDialogResult`).
#[cfg(target_os = "android")]
pub use android::result_raw as android_result_raw;
pub use session::{Consumed, DialogStatus, Session};

/// Which dialog to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogKind {
  /// Pick one file, or several when `multiple`.
  OpenFile { multiple: bool },
  /// Pick a place and name to write to; `suggested_name` pre-fills the name.
  SaveFile { suggested_name: Option<String> },
  /// Pick one folder.
  OpenDirectory,
}

/// A file type the dialog offers to narrow the listing to: a label and the
/// extensions (without the dot) it covers. No filters means every file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogFilter {
  pub name: String,
  pub extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogRequest {
  pub kind: DialogKind,
  pub filters: Vec<DialogFilter>,
  /// Where the dialog opens: a directory path on desktop, a `content://`
  /// URI on Android (a plain path is ignored there). None: the platform's
  /// default.
  pub start_in: Option<String>,
}

/// One entry the user picked: where it is (the string a `flux:fs` handle
/// opens: a path, or a `content://` URI on Android) and the name to show
/// for it (the file name of a path, the provider's display name of a URI).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picked {
  pub location: String,
  pub name: String,
}

/// Why a request got no picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogError {
  /// A dialog is already open: the platform's are modal, one at a time.
  Busy,
  /// This platform (or surface) cannot show a dialog.
  Unavailable,
  /// The platform could not show or finish the dialog.
  Failed(String),
}

impl std::fmt::Display for DialogError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      DialogError::Busy => f.write_str("a dialog is already open"),
      DialogError::Unavailable => f.write_str("no file dialog on this platform"),
      DialogError::Failed(message) => f.write_str(message),
    }
  }
}

/// A request's outcome: the picked entries, empty when the user cancelled
/// (a cancel is an outcome, never an error), or why there are none.
pub type DialogAnswer = Result<Vec<Picked>, DialogError>;

/// What a request is answered through, once, from whatever thread the
/// answer arrives on.
pub type DialogResponder = Box<dyn FnOnce(DialogAnswer) + Send>;

/// Whether this platform can show a file dialog (see the module doc). A
/// dialog that still fails to show rejects at call time with the
/// platform's reason.
pub fn availability() -> Availability {
  availability::of(Facility::Dialog)
}

/// Set by the window's probe (`window::open`), by a call that corrects the
/// fact (`Session`) and by the tests.
pub(crate) fn set_availability(state: Availability) {
  availability::set(Facility::Dialog, state);
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn platform_availability() -> Availability {
  Availability::Available
}

// `SolidRTActivity.dialogsAvailable`'s answer: 0 no document picker (none
// resolves, or only the TV framework's stub, which cancels every request),
// 1 a picker resolves, 2 not known yet (no activity to ask).
#[cfg(target_os = "android")]
const ACTIVITY_NO_PICKER: i32 = 0;
#[cfg(target_os = "android")]
const ACTIVITY_PICKER: i32 = 1;
#[cfg(target_os = "android")]
const ACTIVITY_UNKNOWN: i32 = 2;

#[cfg(target_os = "android")]
pub(crate) fn platform_availability() -> Availability {
  match crate::android::with_activity("dialog availability", |env, activity| {
    let class = env.get_object_class(activity)?;
    env.call_static_method(&class, jni::jni_str!("dialogsAvailable"), jni::jni_sig!("()I"), &[])?.i()
  }) {
    Ok(ACTIVITY_NO_PICKER) => Availability::Unavailable,
    Ok(ACTIVITY_PICKER) => Availability::Available,
    Ok(ACTIVITY_UNKNOWN) => Availability::Unknown,
    Ok(other) => {
      log::warn!("[ore] dialog availability: the activity answered {other}; reporting it unknown");
      Availability::Unknown
    }
    Err(e) => {
      log::warn!("[ore] {e}; reporting file dialogs unknown");
      Availability::Unknown
    }
  }
}

// Linux and the BSDs: SDL's portal backend needs a session bus with the
// portal service on it (activatable through its service file), and falls
// back to zenity on the path. The service file lives under one of the XDG
// data dirs; a session bus address says there is a bus to reach it over.
// zenity on the path is the fallback that works; the portal alone may or
// may not serve the FileChooser, which the portal introspection settles
// (`crate::portal`: its answer if it arrived, else Unknown until it does).
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android", target_os = "ios")))]
pub(crate) fn platform_availability() -> Availability {
  const PORTAL_SERVICE: &str = "dbus-1/services/org.freedesktop.portal.Desktop.service";
  const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
  if on_path("zenity") {
    return Availability::Available;
  }
  let on_bus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
  let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| DEFAULT_DATA_DIRS.to_string());
  let portal = on_bus && data_dirs.split(':').any(|dir| std::path::Path::new(dir).join(PORTAL_SERVICE).is_file());
  if portal {
    crate::portal::dialog_state()
  } else {
    Availability::Unavailable
  }
}

#[cfg(target_os = "ios")]
pub(crate) fn platform_availability() -> Availability {
  Availability::Unavailable
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android", target_os = "ios")))]
fn on_path(program: &str) -> bool {
  let Some(path) = std::env::var_os("PATH") else {
    return false;
  };
  std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

/// The file name of a picked path, for `Picked::name`.
pub(crate) fn name_of_path(path: &str) -> String {
  std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// SDL's filter pattern: the extensions joined by `;` (`"png;jpg"`).
pub(crate) fn sdl_pattern(filter: &DialogFilter) -> String {
  filter.extensions.join(";")
}
