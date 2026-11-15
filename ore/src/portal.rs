//! The xdg-desktop-portal introspection, Linux and the BSDs: whether the
//! session's portal serves the file chooser and the link opener, which
//! settles the dialog and link availability the probes leave Unknown on a
//! session with the portal alone (`crate::dialog`, `crate::link`).
//!
//! One `org.freedesktop.DBus.Introspectable.Introspect` call on the session
//! bus, to `org.freedesktop.portal.Desktop` at
//! `/org/freedesktop/portal/desktop`, the object SDL's own portal backends
//! talk to; its XML is searched for the interfaces
//! `org.freedesktop.portal.FileChooser` (the dialogs) and
//! `org.freedesktop.portal.OpenURI` (the links). SDL decides its dialog
//! driver by the same introspection.
//!
//! It runs once per process, on a thread of its own, started by the first
//! probe that reaches the portal-alone case (`settle_from_portal`): the
//! dialog's at window open, or the link's at the first read. The thread
//! drives a zbus connection on a one-shot current-thread tokio runtime, and
//! gives up after `DEADLINE`. Its answer:
//!
//! - The introspection worked: a facility still Unknown reads Available
//!   when its interface is there, Unavailable when it is not. A state a
//!   call has already corrected is left as it is. A probe that runs after
//!   the answer arrived takes the answer at once (`dialog_state`,
//!   `link_state`).
//! - It failed (no bus, the deadline, an error from the bus): the states
//!   stay Unknown, the first call settles them, and one info line says why.
//!
//! The dialog's answer is recorded on the main thread (`main_thread::run`),
//! where the window's probe records its own: so an answer that arrives while
//! the window opens cannot be overwritten by that probe's Unknown. Only a
//! dialog probe that reached the portal case waits for it; a dialog with no
//! window yet stays Unknown until its window opens.
//!
//! The duration is logged once at info level, with the answer.

use std::sync::{Mutex, MutexGuard, Once, PoisonError};
use std::time::{Duration, Instant};

use crate::availability::{self, Availability, Facility};

/// How long the introspection may take before the probe gives up and
/// leaves the states Unknown: room for the bus to start the portal service
/// on first use (it is activated on demand), while a wedged bus does not
/// hold the probe thread for the life of the process.
const DEADLINE: Duration = Duration::from_secs(5);

/// The portal's bus name, the service that is activated on demand.
const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
/// The object every portal interface lives on.
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
/// The interface SDL's portal dialogs call.
pub(crate) const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
/// The interface SDL's `SDL_OpenURL` tries first.
pub(crate) const OPEN_URI: &str = "org.freedesktop.portal.OpenURI";

/// Which of the interfaces the introspection found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Interfaces {
  pub file_chooser: bool,
  pub open_uri: bool,
}

// The introspection's answer once it arrived, and whether the dialog's
// probe reached the portal case and waits for it. One lock, so a probe
// that reads no answer is sure to be settled by the thread.
struct State {
  answer: Option<Interfaces>,
  dialog_waiting: bool,
}

static STATE: Mutex<State> = Mutex::new(State { answer: None, dialog_waiting: false });

static STARTED: Once = Once::new();

/// Start the introspection, once per process; later calls do nothing. The
/// unit tests set the availability table themselves, so there it never
/// starts: a probe thread landing mid-test would race them.
pub(crate) fn settle_from_portal() {
  if cfg!(test) {
    return;
  }
  STARTED.call_once(|| {
    let spawned = std::thread::Builder::new().name("sol-portal-probe".into()).spawn(probe);
    if let Err(e) = spawned {
      log::info!("[ore] portal introspection not started: {e}; dialog and link stay unknown");
    }
  });
}

/// The dialog's state from the portal, for the dialog's probe in the
/// portal-alone case: the answer if it arrived, else Unknown, and the
/// answer is recorded on the main thread when it arrives. Starts the
/// introspection.
#[cfg(feature = "window")]
pub(crate) fn dialog_state() -> Availability {
  settle_from_portal();
  let mut state = lock();
  match state.answer {
    Some(found) => state_of(found.file_chooser),
    None => {
      state.dialog_waiting = true;
      Availability::Unknown
    }
  }
}

/// The link's state from the portal, for the link's probe in the
/// portal-alone case: the answer if it arrived, else Unknown (the thread
/// records it when it arrives). Starts the introspection.
pub(crate) fn link_state() -> Availability {
  settle_from_portal();
  match lock().answer {
    Some(found) => state_of(found.open_uri),
    None => Availability::Unknown,
  }
}

/// Which interfaces an introspection XML declares: a presence check for
/// each `interface name="..."` element, the form the introspection format
/// fixes.
pub(crate) fn interfaces_in(xml: &str) -> Interfaces {
  let declares = |name: &str| xml.contains(&format!("interface name=\"{name}\""));
  Interfaces { file_chooser: declares(FILE_CHOOSER), open_uri: declares(OPEN_URI) }
}

// The probe thread: introspect, log the duration and the answer, settle.
fn probe() {
  let started = Instant::now();
  let found = introspect();
  let took = started.elapsed().as_millis();
  match found {
    Ok(found) => {
      log::info!(
        "[ore] portal introspection took {took} ms: FileChooser {}, OpenURI {}",
        yes_no(found.file_chooser),
        yes_no(found.open_uri)
      );
      settle(found);
    }
    Err(reason) => {
      log::info!("[ore] portal introspection failed after {took} ms: {reason}; dialog and link stay unknown");
    }
  }
}

// One Introspect call within the deadline, on a runtime of this thread's
// own.
fn introspect() -> Result<Interfaces, String> {
  let runtime =
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("no runtime: {e}"))?;
  let xml = runtime.block_on(async {
    match tokio::time::timeout(DEADLINE, introspect_xml()).await {
      Ok(xml) => xml,
      Err(_) => Err(format!("no answer within {} ms", DEADLINE.as_millis())),
    }
  })?;
  Ok(interfaces_in(&xml))
}

async fn introspect_xml() -> Result<String, String> {
  let connection = zbus::Connection::session().await.map_err(|e| format!("no session bus: {e}"))?;
  let proxy = zbus::fdo::IntrospectableProxy::builder(&connection)
    .destination(PORTAL_DESTINATION)
    .and_then(|builder| builder.path(PORTAL_PATH))
    .map_err(|e| e.to_string())?
    .cache_properties(zbus::proxy::CacheProperties::No)
    .build()
    .await
    .map_err(|e| e.to_string())?;
  proxy.introspect().await.map_err(|e| e.to_string())
}

// Record the answer, then settle what is still Unknown: the link at once,
// the dialog on the main thread if its probe waits (see the module doc).
fn settle(found: Interfaces) {
  let dialog_waiting = {
    let mut state = lock();
    state.answer = Some(found);
    state.dialog_waiting
  };
  settle_unknown(Facility::Link, found.open_uri);
  if dialog_waiting {
    // Queued for the main thread, or run here at once when SDL could not
    // queue it (it has quit); either way the answer is recorded.
    crate::main_thread::run(move |_| settle_unknown(Facility::Dialog, found.file_chooser));
  }
}

// A state a call already corrected is left as it is.
fn settle_unknown(facility: Facility, present: bool) {
  if availability::of(facility) == Availability::Unknown {
    availability::set(facility, state_of(present));
  }
}

fn state_of(present: bool) -> Availability {
  if present {
    Availability::Available
  } else {
    Availability::Unavailable
  }
}

fn yes_no(present: bool) -> &'static str {
  if present {
    "yes"
  } else {
    "no"
  }
}

fn lock() -> MutexGuard<'static, State> {
  STATE.lock().unwrap_or_else(PoisonError::into_inner)
}
