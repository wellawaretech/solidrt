use std::sync::mpsc;

use crate::share::{Session, ShareError, ShareRequest, SharedFile, REQUESTED_LOG_CAP};

fn text(text: &str) -> ShareRequest {
  ShareRequest { text: Some(text.to_string()), ..Default::default() }
}

#[test]
fn a_record_only_session_logs_the_share_and_resolves_without_a_sheet() {
  let session = Session::new();
  session.set_record_only();
  let request = ShareRequest {
    title: Some("Board".to_string()),
    text: Some("My board".to_string()),
    url: None,
    files: vec![SharedFile { location: "share/board.png".to_string(), name: "board.png".to_string() }],
  };
  let (tx, rx) = mpsc::channel();
  session.share(
    request.clone(),
    Box::new(move |result| {
      tx.send(result).expect("send result");
    }),
  );
  assert_eq!(rx.try_recv().expect("answered at once"), Ok(()));
  assert_eq!(session.requested(), vec![request]);
}

#[test]
fn the_log_keeps_the_newest_shares() {
  let session = Session::new();
  session.set_record_only();
  let total = REQUESTED_LOG_CAP * 2;
  for i in 0..total {
    session.share(text(&i.to_string()), Box::new(|_| {}));
  }
  let requested = session.requested();
  assert_eq!(requested.len(), REQUESTED_LOG_CAP);
  assert_eq!(requested.first(), Some(&text(&(total - REQUESTED_LOG_CAP).to_string())));
  assert_eq!(requested.last(), Some(&text(&(total - 1).to_string())));
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_platform_without_a_sheet_rejects_and_still_logs() {
  let session = Session::new();
  let (tx, rx) = mpsc::channel();
  session.share(
    text("hello"),
    Box::new(move |result| {
      tx.send(result).expect("send result");
    }),
  );
  assert_eq!(rx.try_recv().expect("answered at once"), Err(ShareError::Unavailable));
  assert_eq!(session.requested(), vec![text("hello")]);
}
