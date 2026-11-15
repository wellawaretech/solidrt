use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use super::facts_lock;
use crate::availability::{self, Availability, Facility};
use crate::link::{self, LinkError};

#[test]
fn every_facility_has_its_name() {
  let names: Vec<&str> = Facility::ALL.iter().map(|f| f.name()).collect();
  assert_eq!(names, vec!["link", "dialog", "share", "clipboard-image", "haptics", "camera", "microphone", "audio"]);
  assert_eq!(Availability::Available.name(), "available");
  assert_eq!(Availability::Unavailable.name(), "unavailable");
  assert_eq!(Availability::Unknown.name(), "unknown");
}

// The camera is set only once its subsystem starts, which no unit test
// does, so the test owns its state.
#[test]
fn a_change_is_recorded_and_queued_once_and_a_repeat_not_at_all() {
  let _facts = facts_lock();
  availability::set(Facility::Camera, Availability::Unknown);
  availability::drain();

  availability::set(Facility::Camera, Availability::Available);
  assert_eq!(availability::of(Facility::Camera), Availability::Available);
  availability::set(Facility::Camera, Availability::Available);
  availability::set(Facility::Camera, Availability::Unavailable);
  assert_eq!(availability::of(Facility::Camera), Availability::Unavailable);
  assert_eq!(
    availability::drain(),
    vec![(Facility::Camera, Availability::Available), (Facility::Camera, Availability::Unavailable)]
  );
  assert!(availability::drain().is_empty());

  availability::set(Facility::Camera, Availability::Unknown);
  availability::drain();
}

// The camera, the microphone and the audio output read their device lists
// through one mapping: none listed is unavailable, any listed available.
#[test]
fn a_device_list_maps_to_its_availability() {
  assert_eq!(Availability::of_devices(0), Availability::Unavailable);
  assert_eq!(Availability::of_devices(1), Availability::Available);
  assert_eq!(Availability::of_devices(usize::MAX), Availability::Available);
}

// Share, the clipboard image and haptics have a fixed answer per platform,
// probed before the first read returns.
#[test]
fn the_window_free_facilities_are_settled_at_the_first_read() {
  for facility in [Facility::Share, Facility::ClipboardImage, Facility::Haptics] {
    assert_ne!(availability::of(facility), Availability::Unknown, "{}", facility.name());
  }
}

static REFUSED_OPENS: AtomicBool = AtomicBool::new(false);

fn no_opener(_link: &str) -> Result<(), LinkError> {
  REFUSED_OPENS.store(true, Ordering::Relaxed);
  Err(LinkError::Unavailable)
}

fn opener(_link: &str) -> Result<(), LinkError> {
  Ok(())
}

fn open(session: &link::Session, link: &str) -> Result<(), LinkError> {
  let (tx, rx) = mpsc::channel();
  session.open(
    link.to_string(),
    Box::new(move |result| {
      tx.send(result).ok();
    }),
  );
  rx.try_recv().expect("answered at once")
}

#[test]
fn a_link_open_corrects_an_unknown_opener() {
  let _facts = facts_lock();
  let was = link::availability();

  // No opener: the open reaches the platform, its refusal settles the fact,
  // and the next open is refused without reaching it.
  link::set_availability(Availability::Unknown);
  let links = link::Session::with_opener(no_opener);
  assert_eq!(open(&links, "https://example.com"), Err(LinkError::Unavailable));
  assert_eq!(link::availability(), Availability::Unavailable);
  REFUSED_OPENS.store(false, Ordering::Relaxed);
  assert_eq!(open(&links, "https://example.com"), Err(LinkError::Unavailable));
  assert!(!REFUSED_OPENS.load(Ordering::Relaxed));

  // An opener that works settles an Unknown fact as Available.
  link::set_availability(Availability::Unknown);
  let links = link::Session::with_opener(opener);
  assert_eq!(open(&links, "https://example.com"), Ok(()));
  assert_eq!(link::availability(), Availability::Available);

  link::set_availability(was);
  availability::drain();
}

#[cfg(feature = "window")]
mod dialog {
  use std::sync::atomic::{AtomicBool, Ordering};
  use std::sync::mpsc;

  use super::facts_lock;
  use crate::availability::{drain, Availability};
  use crate::dialog::{
    availability, set_availability, DialogAnswer, DialogError, DialogKind, DialogRequest, DialogResponder, Session,
  };

  static SHOWN: AtomicBool = AtomicBool::new(false);

  // A platform with no dialog driver: SDL's "driver unsupported".
  fn no_driver(_request: DialogRequest, respond: DialogResponder) {
    SHOWN.store(true, Ordering::Relaxed);
    respond(Err(DialogError::Unavailable));
  }

  // A platform whose dialog the user cancels.
  fn cancelled(_request: DialogRequest, respond: DialogResponder) {
    SHOWN.store(true, Ordering::Relaxed);
    respond(Ok(Vec::new()));
  }

  fn begin(session: &std::sync::Arc<Session>) -> DialogAnswer {
    let (tx, rx) = mpsc::channel();
    let request = DialogRequest { kind: DialogKind::OpenDirectory, filters: Vec::new(), start_in: None };
    session.begin(
      request,
      Box::new(move |answer| {
        tx.send(answer).ok();
      }),
    );
    rx.try_recv().expect("answered at once")
  }

  #[test]
  fn on_unknown_a_request_shows_and_no_driver_settles_it_unavailable() {
    let _facts = facts_lock();
    set_availability(Availability::Unknown);
    let dialogs = Session::with_show(no_driver);

    SHOWN.store(false, Ordering::Relaxed);
    assert_eq!(begin(&dialogs), Err(DialogError::Unavailable));
    assert!(SHOWN.load(Ordering::Relaxed));
    assert_eq!(availability(), Availability::Unavailable);
    assert_eq!(dialogs.status().availability, Availability::Unavailable);

    // The second request is refused at once, nothing shown.
    SHOWN.store(false, Ordering::Relaxed);
    assert_eq!(begin(&dialogs), Err(DialogError::Unavailable));
    assert!(!SHOWN.load(Ordering::Relaxed));

    drain();
  }

  #[test]
  fn on_unknown_a_cancel_settles_it_available() {
    let _facts = facts_lock();
    set_availability(Availability::Unknown);
    let dialogs = Session::with_show(cancelled);

    SHOWN.store(false, Ordering::Relaxed);
    assert_eq!(begin(&dialogs), Ok(Vec::new()));
    assert!(SHOWN.load(Ordering::Relaxed));
    assert_eq!(availability(), Availability::Available);

    // Left Unavailable, so no other test reaches a platform dialog.
    set_availability(Availability::Unavailable);
    drain();
  }
}
