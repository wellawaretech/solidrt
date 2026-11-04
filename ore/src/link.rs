//! Opening a link: the platform's opener hands it to the app that handles
//! it (a browser for `https:`, the mail app for `mailto:`, the store app
//! for a store link). SDL does it (`SDL_OpenURL`, main thread only, so
//! through the bridge): the xdg-desktop-portal with an `xdg-open` fallback
//! on Linux, `ShellExecute` on Windows, NSWorkspace on macOS, an
//! `ACTION_VIEW` intent through the activity on Android.
//!
//! Success means the opener accepted the link. Android, Windows and macOS
//! refuse a link nothing handles; on Linux the `xdg-open` fallback runs in
//! the background, so there only a missing opener is refused. Nothing
//! reports what the other app does with the link.
//!
//! Availability is a fact of the platform, probed once (`available`):
//! false on a Linux session with neither the portal nor `xdg-open` (a
//! server). An embedder lists the capability from it.
//!
//! The other direction is the app's own scheme, its app id
//! (`com.example.app://settings`): `register` makes this executable its
//! handler (registration.rs), and a second instance the OS starts with a
//! link hands it to the first (instance.rs).
//!
//! `Session` is the session an embedder keeps per engine: every link the
//! app asks to open goes in its log, refused ones included, which a test
//! or an agent reads back, and a session set to record only opens nothing
//! and registers nothing, as a test host's engine does. Like the dialog
//! session, it is where a test takes the platform's place. It also holds
//! what the host knows of the app's own scheme (`OwnScheme`), which the
//! registration reads.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

pub(crate) mod instance;
pub(crate) mod registration;

pub use instance::{hand_off, listen};
pub use registration::register;

/// How many links the log keeps, the newest last: what a test or an agent
/// reads back, without an app that asks for links in a loop growing it.
pub(crate) const REQUESTED_LOG_CAP: usize = 64;

/// Why a link was not opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
  /// This platform (or session) has no opener.
  Unavailable,
  /// The platform refused the link, or could not hand it over.
  Failed(String),
}

impl std::fmt::Display for LinkError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      LinkError::Unavailable => f.write_str("no link opener on this platform"),
      LinkError::Failed(message) => f.write_str(message),
    }
  }
}

/// What an open is answered through, once, from the thread it finished on.
pub type LinkResponder = Box<dyn FnOnce(Result<(), LinkError>) + Send>;

/// Whether this platform can open a link at all (see the module doc).
pub fn available() -> bool {
  static AVAILABLE: OnceLock<bool> = OnceLock::new();
  *AVAILABLE.get_or_init(platform_available)
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "android"))]
fn platform_available() -> bool {
  true
}

// Linux and the BSDs: SDL tries the portal's OpenURI over the session bus
// first (activatable through the portal's service file under one of the
// XDG data dirs), then spawns `xdg-open` from the path.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn platform_available() -> bool {
  const PORTAL_SERVICE: &str = "dbus-1/services/org.freedesktop.portal.Desktop.service";
  const DEFAULT_DATA_DIRS: &str = "/usr/local/share:/usr/share";
  let on_bus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
  let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| DEFAULT_DATA_DIRS.to_string());
  let portal = on_bus && data_dirs.split(':').any(|dir| std::path::Path::new(dir).join(PORTAL_SERVICE).is_file());
  portal || on_path("xdg-open")
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn on_path(program: &str) -> bool {
  let Some(path) = std::env::var_os("PATH") else {
    return false;
  };
  std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

/// The shortest scheme a link may have: a one-letter "scheme" is a Windows
/// drive (`C:\file`), which the opener would run as a local file.
const MIN_SCHEME_LEN: usize = 2;

/// The scheme of `link`, by RFC 3986's grammar: a letter, then letters,
/// digits, `+`, `-` or `.`, up to the first `:`, at least
/// `MIN_SCHEME_LEN` long. None for a link without one (`/settings`,
/// `example.com`) or with a drive letter in its place (`C:\file`).
pub fn scheme(link: &str) -> Option<&str> {
  let (scheme, _) = link.split_once(':')?;
  let mut chars = scheme.chars();
  let first = chars.next()?;
  (scheme.len() >= MIN_SCHEME_LEN
    && first.is_ascii_alphabetic()
    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
  .then_some(scheme)
}

/// Whether `arg` is a link of the app's own scheme: `<scheme>:` followed by
/// anything, the scheme compared case-insensitively as schemes are.
pub fn own_link(scheme: &str, arg: &str) -> bool {
  let n = scheme.len();
  arg.len() > n && arg.is_char_boundary(n) && arg[..n].eq_ignore_ascii_case(scheme) && arg.as_bytes()[n] == b':'
}

/// Open `link` now. Main thread only: reach it through `Session::open`.
///
/// Called through `sdl3::sys`, not `sdl3::url::open_url`: on Android
/// `SDL_OpenURL` fails without setting an error (the activity's `openURL`
/// returns false when no app handles the link), so the error is cleared
/// first and a failure with none set is the refusal; the crate would read
/// back a stale message from an earlier, unrelated call.
fn open_now(link: &str) -> Result<(), LinkError> {
  let link =
    std::ffi::CString::new(link).map_err(|_| LinkError::Failed("the link contains a NUL character".to_string()))?;
  // SAFETY: both calls take no pointers but `link`, a valid C string that
  // outlives the call.
  let opened = unsafe {
    sdl3::sys::error::SDL_ClearError();
    sdl3::sys::misc::SDL_OpenURL(link.as_ptr())
  };
  if opened {
    return Ok(());
  }
  let error = crate::sdl_error();
  Err(LinkError::Failed(if error.is_empty() { "no app on this device handles the link".to_string() } else { error }))
}

/// Who answers the app's own scheme in this host, as the host knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnScheme {
  /// No app identity (headless flux, a dev run of the runner): there is
  /// nothing to register, and `register_handler` says so.
  Unknown,
  /// This host is not the scheme's handler (the dev client, the player):
  /// `register_handler` warns and writes nothing, so an app that registers
  /// at startup runs unchanged.
  NotHandler,
  /// A packed app: `register_handler` makes this executable the handler.
  /// `display_name` is what the OS shows for it.
  Own { scheme: String, display_name: String },
}

/// The link session of one engine (see the module doc).
pub struct Session {
  record_only: AtomicBool,
  requested: Mutex<VecDeque<String>>,
  own_scheme: Mutex<OwnScheme>,
}

impl Session {
  pub fn new() -> Arc<Session> {
    Arc::new(Session {
      record_only: AtomicBool::new(false),
      requested: Mutex::new(VecDeque::new()),
      own_scheme: Mutex::new(OwnScheme::Unknown),
    })
  }

  /// Never reach the platform in this session: every open is logged and
  /// resolves, and a registration writes nothing (a test host's engine, a
  /// render).
  pub fn set_record_only(&self) {
    self.record_only.store(true, Ordering::Relaxed);
  }

  pub fn is_record_only(&self) -> bool {
    self.record_only.load(Ordering::Relaxed)
  }

  /// The links the app asked to open in this session, oldest first, the
  /// last `REQUESTED_LOG_CAP` of them: refused ones too, so the log says
  /// which link a button tried, whatever the platform answered.
  pub fn requested(&self) -> Vec<String> {
    self.requested.lock().unwrap_or_else(PoisonError::into_inner).iter().cloned().collect()
  }

  /// What the host knows of the app's own scheme; `Unknown` until set.
  pub fn set_own_scheme(&self, own: OwnScheme) {
    *self.own_scheme.lock().unwrap_or_else(PoisonError::into_inner) = own;
  }

  /// Make this executable the handler for the app's own scheme, as
  /// `OwnScheme` says (see there); a record-only session writes nothing.
  pub fn register_handler(&self) -> Result<(), String> {
    if self.is_record_only() {
      return Ok(());
    }
    let own = self.own_scheme.lock().unwrap_or_else(PoisonError::into_inner).clone();
    match own {
      OwnScheme::Unknown => Err("this run has no app identity (not a packed app); nothing to register".to_string()),
      OwnScheme::NotHandler => {
        log::warn!("[ore] Nothing registered: this host (a dev client, the player) is not the app's scheme handler; a packed app registers its own scheme");
        Ok(())
      }
      OwnScheme::Own { scheme, display_name } => register(&scheme, &display_name),
    }
  }

  /// Open `link` on the main thread and answer through `respond`, logging
  /// it first; a record-only session answers at once.
  pub fn open(&self, link: String, respond: LinkResponder) {
    {
      let mut requested = self.requested.lock().unwrap_or_else(PoisonError::into_inner);
      if requested.len() == REQUESTED_LOG_CAP {
        requested.pop_front();
      }
      requested.push_back(link.clone());
    }
    if self.is_record_only() {
      return respond(Ok(()));
    }
    if !available() {
      return respond(Err(LinkError::Unavailable));
    }
    crate::main_thread::run(move |on_main| {
      respond(on_main.map_err(LinkError::Failed).and_then(|()| open_now(&link)));
    });
  }
}
