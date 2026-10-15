use std::sync::mpsc;

use crate::dialog::{
  name_of_path, sdl_pattern, set_available, DialogError, DialogFilter, DialogKind, DialogRequest, Dialogs, Picked,
};

#[test]
fn filter_pattern_joins_extensions_for_sdl() {
  let filter = DialogFilter { name: "Images".to_string(), extensions: vec!["png".to_string(), "jpg".to_string()] };
  assert_eq!(sdl_pattern(&filter), "png;jpg");
  let one = DialogFilter { name: "Text".to_string(), extensions: vec!["txt".to_string()] };
  assert_eq!(sdl_pattern(&one), "txt");
}

#[test]
fn picked_name_is_the_file_name_of_a_path() {
  assert_eq!(name_of_path("/home/u/docs/report.csv"), "report.csv");
  assert_eq!(name_of_path("report.csv"), "report.csv");
}

#[test]
fn picked_name_falls_back_to_the_whole_string() {
  // A location without a file name component (a root) keeps the string.
  assert_eq!(name_of_path("/"), "/");
}

fn request(kind: DialogKind) -> DialogRequest {
  DialogRequest { kind, filters: Vec::new(), start_in: None }
}

fn picked(location: &str) -> Picked {
  Picked { location: location.to_string(), name: location.to_string() }
}

#[test]
fn an_answer_given_ahead_is_what_the_next_request_takes() {
  let dialogs = Dialogs::new();
  let (told_tx, told_rx) = mpsc::channel();
  let consumed = Box::new(move |request| {
    told_tx.send(request).ok();
  });
  assert!(!dialogs.answer(vec![picked("a.txt")], Some(consumed)));
  assert_eq!(dialogs.status().queued, 1);

  let (tx, rx) = mpsc::channel();
  let respond = Box::new(move |answer| {
    tx.send(answer).ok();
  });
  dialogs.begin(request(DialogKind::OpenDirectory), respond, |_, _| panic!("a queued answer shows no dialog"));
  assert_eq!(rx.recv().expect("answered"), Ok(vec![picked("a.txt")]));
  assert_eq!(told_rx.recv().expect("told").kind, DialogKind::OpenDirectory);
  let status = dialogs.status();
  assert_eq!(status.queued, 0);
  assert!(status.pending.is_none());
  assert_eq!(status.last.map(|(_, answer)| answer), Some(Some(Ok(vec![picked("a.txt")]))));
}

#[test]
fn under_answers_only_a_request_waits_and_a_second_one_is_refused() {
  let dialogs = Dialogs::new();
  dialogs.set_answers_only();
  let (tx, rx) = mpsc::channel();
  let respond = Box::new(move |answer| {
    tx.send(answer).ok();
  });
  dialogs
    .begin(request(DialogKind::OpenFile { multiple: true }), respond, |_, _| panic!("answers only shows no dialog"));
  assert!(rx.try_recv().is_err());
  assert_eq!(dialogs.status().pending.map(|r| r.kind), Some(DialogKind::OpenFile { multiple: true }));

  let (tx2, rx2) = mpsc::channel();
  let respond2 = Box::new(move |answer| {
    tx2.send(answer).ok();
  });
  dialogs.begin(request(DialogKind::OpenDirectory), respond2, |_, _| panic!("a refused request shows no dialog"));
  assert_eq!(rx2.recv().expect("refused"), Err(DialogError::Busy));
  // The first still waits, and the refused one did not become the last.
  assert_eq!(dialogs.status().last.map(|(r, _)| r.kind), Some(DialogKind::OpenFile { multiple: true }));

  assert!(dialogs.answer(Vec::new(), None));
  assert_eq!(rx.recv().expect("settled"), Ok(Vec::new()));
  assert!(dialogs.status().pending.is_none());
}

// The platform path, in one test since availability is process-wide.
#[test]
fn the_platform_shows_when_available_and_its_late_answer_is_dropped() {
  let dialogs = Dialogs::new();

  set_available(false);
  let (tx, rx) = mpsc::channel();
  let respond = Box::new(move |answer| {
    tx.send(answer).ok();
  });
  dialogs.begin(request(DialogKind::OpenDirectory), respond, |_, _| panic!("nothing to show on"));
  assert_eq!(rx.recv().expect("refused"), Err(DialogError::Unavailable));
  assert_eq!(dialogs.status().last.map(|(_, answer)| answer), Some(Some(Err(DialogError::Unavailable))));

  set_available(true);
  let (tx, rx) = mpsc::channel();
  let respond = Box::new(move |answer| {
    tx.send(answer).ok();
  });
  let (shown_tx, shown_rx) = mpsc::channel();
  dialogs.begin(request(DialogKind::OpenFile { multiple: false }), respond, move |request, respond| {
    shown_tx.send((request, respond)).ok();
  });
  let (shown, platform_respond) = shown_rx.recv().expect("shown");
  assert_eq!(shown.kind, DialogKind::OpenFile { multiple: false });
  assert!(dialogs.status().pending.is_some());

  // An outside answer settles the request first; the platform's own,
  // arriving later, is dropped and touches nothing.
  assert!(dialogs.answer(vec![picked("b.txt")], None));
  assert_eq!(rx.recv().expect("settled"), Ok(vec![picked("b.txt")]));
  platform_respond(Ok(Vec::new()));
  assert_eq!(dialogs.status().last.map(|(_, answer)| answer), Some(Some(Ok(vec![picked("b.txt")]))));

  // Left alone, the platform's answer is the request's.
  let (tx, rx) = mpsc::channel();
  let respond = Box::new(move |answer| {
    tx.send(answer).ok();
  });
  let (shown_tx, shown_rx) = mpsc::channel();
  dialogs.begin(request(DialogKind::OpenDirectory), respond, move |request, respond| {
    shown_tx.send((request, respond)).ok();
  });
  let (_, platform_respond) = shown_rx.recv().expect("shown");
  platform_respond(Ok(vec![picked("docs")]));
  assert_eq!(rx.recv().expect("answered"), Ok(vec![picked("docs")]));
  assert!(dialogs.status().pending.is_none());
  set_available(false);
}
