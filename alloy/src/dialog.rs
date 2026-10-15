//! System file dialogs: the platform's own open, save and folder pickers.
//! A dialog is shown from the platform loop's thread as a command
//! (`AlloyCommand::ShowDialog`) and answered through a responder once the
//! user is done, the clipboard's round trip with a later answer. Desktop
//! goes through SDL (the xdg-desktop-portal file chooser with a zenity
//! fallback on Linux, IFileDialog on Windows, NSOpenPanel as a sheet on
//! the app window on macOS). Android goes through the activity's own
//! document intents (`SolidRTActivity.showDialog`): SDL's Android backend
//! takes no persistable grant on what was picked and has no folder picker,
//! so the activity owns the intents and reports the result over JNI
//! (`android_result`). What comes back is a location string a file handle
//! opens later: a path on desktop, a `content://` URI on Android.
//!
//! Availability is a fact of the platform read once at setup (`available`):
//! false on a headless surface, on a platform without a backend (iOS), on
//! a Linux session with neither the portal nor zenity, and on an Android
//! without a document picker (a TV). An embedder lists the capability from
//! it, so an app can hide its Open and Save buttons up front.
//!
//! `Dialogs` is the session an embedder keeps per engine: one dialog at a
//! time (the platform's are modal), the request waiting for its answer, the
//! last request made with its answer, and the answers a test or an agent
//! gives in the user's place. An answer given ahead is what the next
//! request takes instead of showing a dialog; an answer arriving while a
//! request waits settles it at once, and a native dialog still open then
//! has its own answer dropped (SDL cannot dismiss one it showed); a session
//! set to answers only never shows anything but waits on the queue, as a
//! test host's engine does. Like a test microphone or a still camera, the
//! session is where a test takes the platform's place.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

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

/// Told, with the request, once a request took an answer given ahead.
pub type Consumed = Box<dyn FnOnce(DialogRequest) + Send>;

static AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Whether this platform can show a file dialog at all (see the module
/// doc). Read once at setup; a dialog that still fails to show rejects at
/// call time with the platform's reason.
pub fn dialogs_available() -> bool {
  AVAILABLE.load(Ordering::Relaxed)
}

/// Probe the platform once the window exists; `headless` means a surface
/// no dialog could attach to.
pub(crate) fn probe(headless: bool) {
  set_available(!headless && platform_available());
}

pub(crate) fn set_available(available: bool) {
  AVAILABLE.store(available, Ordering::Relaxed);
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn platform_available() -> bool {
  true
}

#[cfg(target_os = "android")]
fn platform_available() -> bool {
  match crate::sdl_utils::with_android_activity("dialog availability", |env, activity| {
    let class = env.get_object_class(activity)?;
    env.call_static_method(&class, jni::jni_str!("dialogsAvailable"), jni::jni_sig!("()Z"), &[])?.z()
  }) {
    Ok(available) => available,
    Err(e) => {
      log::warn!("[alloy] {e}; reporting no file dialogs");
      false
    }
  }
}

// Linux and the BSDs: SDL's portal backend needs a session bus with the
// portal service on it (activatable through its service file), and falls
// back to zenity on the path. The service file lives under one of the XDG
// data dirs; a session bus address says there is a bus to reach it over.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android", target_os = "ios")))]
fn platform_available() -> bool {
  const PORTAL_SERVICE: &str = "dbus-1/services/org.freedesktop.portal.Desktop.service";
  const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
  let on_bus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
  let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| DEFAULT_DATA_DIRS.to_string());
  let portal = on_bus && data_dirs.split(':').any(|dir| std::path::Path::new(dir).join(PORTAL_SERVICE).is_file());
  portal || on_path("zenity")
}

#[cfg(target_os = "ios")]
fn platform_available() -> bool {
  false
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

// The request waiting for its answer: parked on the queue (answers only),
// or on the platform's dialog, whose own answer carries the id and is
// dropped if an outside answer settled the request first.
struct Pending {
  id: u64,
  request: DialogRequest,
  respond: DialogResponder,
}

// An answer given ahead, with whom to tell once a request took it.
struct Queued {
  picked: Vec<Picked>,
  consumed: Option<Consumed>,
}

struct Session {
  answers_only: bool,
  queue: VecDeque<Queued>,
  pending: Option<Pending>,
  /// The last request made, with its answer once it has one.
  last: Option<(DialogRequest, Option<DialogAnswer>)>,
  next_id: u64,
}

/// What an embedder's control surface reports about a session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DialogStatus {
  /// Whether this platform can show a dialog (`dialogs_available`).
  pub available: bool,
  /// The request waiting for an answer.
  pub pending: Option<DialogRequest>,
  /// The last request made, with its answer once it has one.
  pub last: Option<(DialogRequest, Option<DialogAnswer>)>,
  /// How many answers given ahead wait for a request.
  pub queued: usize,
}

/// The dialog session of one engine (see the module doc).
pub struct Dialogs {
  session: Mutex<Session>,
}

// What `begin` decided under the lock, acted on outside it (a responder may
// reach back into the session).
enum Next {
  Refuse(DialogResponder, DialogError),
  Consumed(DialogResponder, Vec<Picked>, Option<Consumed>),
  Parked(Option<u64>),
}

impl Dialogs {
  pub fn new() -> Arc<Dialogs> {
    Arc::new(Dialogs {
      session: Mutex::new(Session {
        answers_only: false,
        queue: VecDeque::new(),
        pending: None,
        last: None,
        next_id: 1,
      }),
    })
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Session> {
    self.session.lock().unwrap_or_else(PoisonError::into_inner)
  }

  /// Never show a dialog in this session: every request waits on the queue
  /// (a test host's engine, which has no surface to show one on and whose
  /// test answers through the harness).
  pub fn set_answers_only(&self) {
    self.lock().answers_only = true;
  }

  /// Begin a request: refuse it while another waits, answer it from the
  /// queue, park it for an answer (answers only), or hand it to `show`,
  /// the platform's dialog, with the responder its answer goes through.
  /// `respond` is called once, with the picks or why there are none, from
  /// whatever thread the answer arrives on.
  pub fn begin(
    self: &Arc<Self>,
    request: DialogRequest,
    respond: DialogResponder,
    show: impl FnOnce(DialogRequest, DialogResponder),
  ) {
    let next = {
      let mut s = self.lock();
      if s.pending.is_some() {
        Next::Refuse(respond, DialogError::Busy)
      } else if let Some(queued) = s.queue.pop_front() {
        s.last = Some((request.clone(), Some(Ok(queued.picked.clone()))));
        Next::Consumed(respond, queued.picked, queued.consumed)
      } else if !s.answers_only && !dialogs_available() {
        s.last = Some((request.clone(), Some(Err(DialogError::Unavailable))));
        Next::Refuse(respond, DialogError::Unavailable)
      } else {
        let id = s.next_id;
        s.next_id += 1;
        let shown = !s.answers_only;
        s.last = Some((request.clone(), None));
        s.pending = Some(Pending { id, request: request.clone(), respond });
        Next::Parked(shown.then_some(id))
      }
    };
    match next {
      Next::Refuse(respond, error) => respond(Err(error)),
      Next::Consumed(respond, picked, consumed) => {
        respond(Ok(picked));
        if let Some(consumed) = consumed {
          consumed(request);
        }
      }
      Next::Parked(None) => {}
      Next::Parked(Some(id)) => {
        let session = self.clone();
        show(request, Box::new(move |answer| session.settle(id, answer)));
      }
    }
  }

  // The platform's answer to request `id`: taken if that request still
  // waits, dropped if an outside answer settled it (or a later request
  // waits under another id).
  fn settle(&self, id: u64, answer: DialogAnswer) {
    let taken = {
      let mut s = self.lock();
      if s.pending.as_ref().map_or(false, |p| p.id == id) {
        record(&mut s, &answer);
        s.pending.take()
      } else {
        None
      }
    };
    if let Some(pending) = taken {
      (pending.respond)(answer);
    }
  }

  /// Answer the waiting request, or keep the answer for the next one.
  /// `consumed` is told with the request once one took the answer (at once
  /// when one is waiting). Returns whether a request was waiting.
  pub fn answer(&self, picked: Vec<Picked>, consumed: Option<Consumed>) -> bool {
    let pending = {
      let mut s = self.lock();
      match s.pending.take() {
        Some(pending) => {
          record(&mut s, &Ok(picked.clone()));
          pending
        }
        None => {
          s.queue.push_back(Queued { picked, consumed });
          return false;
        }
      }
    };
    (pending.respond)(Ok(picked));
    if let Some(consumed) = consumed {
      consumed(pending.request);
    }
    true
  }

  pub fn status(&self) -> DialogStatus {
    let s = self.lock();
    DialogStatus {
      available: dialogs_available(),
      pending: s.pending.as_ref().map(|p| p.request.clone()),
      last: s.last.clone(),
      queued: s.queue.len(),
    }
  }
}

fn record(s: &mut Session, answer: &DialogAnswer) {
  if let Some((_, slot)) = s.last.as_mut() {
    *slot = Some(answer.clone());
  }
}

/// Show the dialog on the loop thread (desktop: SDL, with the app window as
/// the parent; Android: the activity's intents). The responder runs on
/// whatever thread the platform answers from, once.
#[cfg(not(target_os = "android"))]
pub(crate) fn show(window: &sdl3::video::Window, request: DialogRequest, respond: DialogResponder) {
  use sdl3::dialog::{show_open_file_dialog, show_open_folder_dialog, show_save_file_dialog, DialogFileFilter};

  // SDL's callback type is `Fn`, called once; the responder is `FnOnce`.
  // It stays reachable from here too: when SDL refuses the request before
  // showing anything, the callback is dropped unrun and the caller is
  // answered directly.
  let respond = Arc::new(Mutex::new(Some(respond)));
  let callback_respond = respond.clone();
  let callback: sdl3::dialog::DialogCallback = Box::new(move |result, _filter| {
    let Some(respond) = callback_respond.lock().unwrap_or_else(PoisonError::into_inner).take() else {
      return;
    };
    respond(match result {
      Ok(paths) => Ok(
        paths
          .into_iter()
          .map(|p| {
            let location = p.to_string_lossy().into_owned();
            Picked { name: name_of_path(&location), location }
          })
          .collect(),
      ),
      Err(sdl3::dialog::DialogError::Canceled) => Ok(Vec::new()),
      Err(e) => Err(DialogError::Failed(format!("dialog: {e}"))),
    });
  });

  let patterns: Vec<String> = request.filters.iter().map(sdl_pattern).collect();
  let filters: Vec<DialogFileFilter> = request
    .filters
    .iter()
    .zip(patterns.iter())
    .map(|(f, pattern)| DialogFileFilter { name: f.name.as_str(), pattern: pattern.as_str() })
    .collect();
  let start_in = request.start_in.as_deref();
  let shown = match request.kind {
    DialogKind::OpenFile { multiple } => show_open_file_dialog(&filters, start_in, multiple, window, callback),
    DialogKind::SaveFile { suggested_name } => {
      // SDL's save dialog takes one default location, a file path: the
      // suggested name is joined onto the start directory (or stands on
      // its own, which the platform resolves against its default folder).
      let default = match (start_in, suggested_name) {
        (Some(dir), Some(name)) => Some(std::path::Path::new(dir).join(name).to_string_lossy().into_owned()),
        (Some(dir), None) => Some(dir.to_string()),
        (None, Some(name)) => Some(name),
        (None, None) => None,
      };
      show_save_file_dialog(&filters, default.as_deref(), window, callback)
    }
    DialogKind::OpenDirectory => {
      show_open_folder_dialog(start_in, false, window, callback);
      Ok(())
    }
  };
  if let Err(e) = shown {
    // The request never reached SDL (a NUL in a filter) and the callback
    // will not run; SDL reports every other failure through the callback.
    if let Some(respond) = respond.lock().unwrap_or_else(PoisonError::into_inner).take() {
      respond(Err(DialogError::Failed(format!("dialog: {e}"))));
    }
  }
}

#[cfg(target_os = "android")]
mod android {
  use super::{DialogError, DialogKind, DialogRequest, DialogResponder, Picked};
  use std::collections::HashMap;
  use std::sync::atomic::{AtomicU64, Ordering};
  use std::sync::Mutex;

  // The dialogs the activity has open, by request id, each with the
  // responder its result settles.
  static PENDING: Mutex<Option<HashMap<u64, DialogResponder>>> = Mutex::new(None);
  static NEXT_ID: AtomicU64 = AtomicU64::new(1);

  // The `kind` argument of `SolidRTActivity.showDialog`.
  const KIND_OPEN_FILE: i32 = 0;
  const KIND_SAVE_FILE: i32 = 1;
  const KIND_OPEN_DIRECTORY: i32 = 2;

  fn take(id: u64) -> Option<DialogResponder> {
    PENDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut().and_then(|map| map.remove(&id))
  }

  pub(super) fn show(request: DialogRequest, respond: DialogResponder) {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    PENDING
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
      .get_or_insert_with(HashMap::new)
      .insert(id, respond);
    let (kind, multiple, suggested_name) = match request.kind {
      DialogKind::OpenFile { multiple } => (KIND_OPEN_FILE, multiple, None),
      DialogKind::SaveFile { suggested_name } => (KIND_SAVE_FILE, false, suggested_name),
      DialogKind::OpenDirectory => (KIND_OPEN_DIRECTORY, false, None),
    };
    let patterns: Vec<String> = request.filters.iter().map(super::sdl_pattern).collect();
    let launched = crate::sdl_utils::with_android_activity("dialog", |env, activity| {
      let empty = env.new_string("")?;
      let filters = jni::objects::JObjectArray::<jni::objects::JString>::new(env, patterns.len(), &empty)?;
      for (i, pattern) in patterns.iter().enumerate() {
        let s = env.new_string(pattern)?;
        filters.set_element(env, i, &s)?;
      }
      let suggested = env.new_string(suggested_name.as_deref().unwrap_or(""))?;
      let start_in = env.new_string(request.start_in.as_deref().unwrap_or(""))?;
      let class = env.get_object_class(activity)?;
      env
        .call_static_method(
          &class,
          jni::jni_str!("showDialog"),
          jni::jni_sig!("(JI[Ljava/lang/String;ZLjava/lang/String;Ljava/lang/String;)Z"),
          &[
            jni::objects::JValue::Long(id as i64),
            jni::objects::JValue::Int(kind),
            jni::objects::JValue::Object(&filters),
            jni::objects::JValue::Bool(multiple),
            jni::objects::JValue::Object(&suggested),
            jni::objects::JValue::Object(&start_in),
          ],
        )?
        .z()
    });
    let failed = match launched {
      Ok(true) => None,
      Ok(false) => Some("dialog: no document picker on this device".to_string()),
      Err(e) => Some(e),
    };
    if let Some(message) = failed {
      if let Some(respond) = take(id) {
        respond(Err(DialogError::Failed(message)));
      }
    }
  }

  /// The activity's answer to dialog `id` (`nativeDialogResult`): the
  /// picked URIs with their display names, both empty on a cancel.
  pub fn result(id: u64, locations: Vec<String>, names: Vec<String>) {
    let Some(respond) = take(id) else {
      log::warn!("[alloy] dialog result for unknown request {id}");
      return;
    };
    let picked = locations
      .into_iter()
      .zip(names.into_iter().chain(std::iter::repeat(String::new())))
      .map(|(location, name)| {
        let name = if name.is_empty() { super::name_of_path(&location) } else { name };
        Picked { location, name }
      })
      .collect();
    respond(Ok(picked));
  }

  /// `result` from the raw JNI arguments the activity's native method
  /// receives (the env, then the two `String[]`), for the embedder's
  /// exported symbol to forward as the opaque pointers it gets.
  pub fn result_raw(env: *mut core::ffi::c_void, id: i64, uris: *mut core::ffi::c_void, names: *mut core::ffi::c_void) {
    let env = env as *mut jni::sys::JNIEnv;
    let (uris, names) = (uris as jni::sys::jobjectArray, names as jni::sys::jobjectArray);
    let decoded = decode_strings(env, uris).and_then(|uris| decode_strings(env, names).map(|names| (uris, names)));
    match decoded {
      Ok((uris, names)) => result(id as u64, uris, names),
      Err(e) => {
        log::warn!("[alloy] dialog result for request {id} could not be read: {e}");
        if let Some(respond) = take(id as u64) {
          respond(Err(DialogError::Failed(format!("dialog: the result could not be read: {e}"))));
        }
      }
    }
  }

  fn decode_strings(env: *mut jni::sys::JNIEnv, array: jni::sys::jobjectArray) -> Result<Vec<String>, String> {
    if env.is_null() {
      return Err("no JNI env".to_string());
    }
    if array.is_null() {
      return Ok(Vec::new());
    }
    let mut unowned = unsafe { jni::EnvUnowned::from_raw(env) };
    let outcome = unowned
      .with_env(|env| -> Result<Vec<String>, jni::errors::Error> {
        let array = unsafe { jni::objects::JObjectArray::<jni::objects::JString>::from_raw(env, array) };
        crate::sdl_utils::java_strings(env, &array)
      })
      .into_outcome();
    match outcome {
      jni::Outcome::Ok(strings) => Ok(strings),
      jni::Outcome::Err(e) => Err(e.to_string()),
      jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
    }
  }
}

#[cfg(target_os = "android")]
pub(crate) fn show(_window: &sdl3::video::Window, request: DialogRequest, respond: DialogResponder) {
  android::show(request, respond);
}

/// The activity's answer to a dialog, from the embedder's exported JNI
/// symbol (`Java_com_solidrt_app_SolidRTActivity_nativeDialogResult`).
#[cfg(target_os = "android")]
pub use android::result_raw as android_result_raw;
