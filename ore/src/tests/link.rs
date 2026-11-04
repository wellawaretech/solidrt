use std::sync::mpsc;

use crate::link::{scheme, Opener, REQUESTED_LOG_CAP};

#[test]
fn a_scheme_is_read_by_the_rfc_grammar() {
  assert_eq!(scheme("https://example.com"), Some("https"));
  assert_eq!(scheme("mailto:someone@example.com"), Some("mailto"));
  assert_eq!(scheme("com.example.app://settings"), Some("com.example.app"));
  assert_eq!(scheme("market://details?id=com.example"), Some("market"));
  assert_eq!(scheme("/settings"), None);
  assert_eq!(scheme("example.com"), None);
  assert_eq!(scheme("://nothing"), None);
  assert_eq!(scheme("1http://example.com"), None);
  assert_eq!(scheme("my app://x"), None);
}

#[test]
fn a_drive_letter_is_not_a_scheme() {
  assert_eq!(scheme("C:\\Windows\\notepad.exe"), None);
  assert_eq!(scheme("d:/notes.txt"), None);
  assert_eq!(scheme("ws://example.com"), Some("ws"));
}

#[test]
fn a_record_only_session_logs_the_link_and_resolves_without_opening() {
  let opener = Opener::new();
  opener.set_record_only();
  let (tx, rx) = mpsc::channel();
  opener.open(
    "https://example.com/credits".to_string(),
    Box::new(move |result| {
      tx.send(result).expect("send result");
    }),
  );
  assert_eq!(rx.try_recv().expect("answered at once"), Ok(()));
  assert_eq!(opener.requested(), vec!["https://example.com/credits".to_string()]);
}

#[test]
fn the_log_keeps_the_newest_links() {
  let opener = Opener::new();
  opener.set_record_only();
  let total = REQUESTED_LOG_CAP * 2;
  for i in 0..total {
    opener.open(format!("https://example.com/{i}"), Box::new(|_| {}));
  }
  let requested = opener.requested();
  assert_eq!(requested.len(), REQUESTED_LOG_CAP);
  assert_eq!(requested.first().cloned(), Some(format!("https://example.com/{}", total - REQUESTED_LOG_CAP)));
  assert_eq!(requested.last().cloned(), Some(format!("https://example.com/{}", total - 1)));
}
