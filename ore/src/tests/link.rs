use std::sync::mpsc;

use crate::link::{own_link, scheme, OwnScheme, Session, REQUESTED_LOG_CAP};

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
  let opener = Session::new();
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
  let opener = Session::new();
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

#[test]
fn own_link_matches_the_app_scheme_only() {
  assert!(own_link("com.example.app", "com.example.app://settings"));
  assert!(own_link("com.example.app", "com.example.app:/settings"));
  assert!(own_link("com.example.app", "COM.Example.App://settings"), "schemes compare case-insensitively");
  assert!(!own_link("com.example.app", "com.example.app"), "a scheme needs its colon");
  assert!(!own_link("com.example.app", "com.example.application://x"), "a longer id is another app");
  assert!(!own_link("com.example.app", "https://example.com"));
  assert!(!own_link("com.example.app", "--flag"));
  assert!(!own_link("com.example.app", ""));
  assert!(!own_link("com.example.app", "com.exampl\u{e9}app://x"), "a multi-byte char inside the id span is no match");
}

#[test]
fn a_session_without_an_app_identity_registers_nothing() {
  let session = Session::new();
  assert!(session.register_handler().is_err(), "an unknown scheme has nothing to register");
  session.set_own_scheme(OwnScheme::NotHandler);
  assert_eq!(session.register_handler(), Ok(()), "a host that is not the handler warns and returns");
}

#[test]
fn a_record_only_session_writes_no_registration() {
  let session = Session::new();
  session.set_record_only();
  session.set_own_scheme(OwnScheme::Own {
    scheme: "com.example.test-never-written".to_string(),
    display_name: "Never written".to_string(),
  });
  assert_eq!(session.register_handler(), Ok(()));
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
mod freedesktop {
  use crate::link::registration::freedesktop::{desktop_entry, exec_quote, with_default};
  use std::path::Path;

  #[test]
  fn exec_quote_quotes_only_what_the_specification_reserves() {
    assert_eq!(exec_quote("/opt/app/bin"), "/opt/app/bin");
    assert_eq!(exec_quote("/opt/app-1.0_x64/bin"), "/opt/app-1.0_x64/bin");
    assert_eq!(exec_quote("/tmp/a b"), "\"/tmp/a b\"");
    assert_eq!(exec_quote("/tmp/it's"), "\"/tmp/it's\"");
    assert_eq!(exec_quote("/tmp/q\"b`$\\"), "\"/tmp/q\\\"b\\`\\$\\\\\"");
    assert_eq!(exec_quote("/tmp/100%"), "\"/tmp/100%%\"");
  }

  #[test]
  fn desktop_entry_names_the_scheme_handler() {
    let text = desktop_entry("com.example.app", "Example App", Path::new("/opt/example/app"));
    assert_eq!(
      text,
      "[Desktop Entry]\nType=Application\nName=Example App\nExec=/opt/example/app %u\nTerminal=false\nMimeType=x-scheme-handler/com.example.app;\n"
    );
  }

  #[test]
  fn with_default_creates_both_sections_in_an_empty_file() {
    let text = with_default("", "x-scheme-handler/com.example.app", "com.example.app.desktop");
    assert_eq!(
      text,
      "[Default Applications]\nx-scheme-handler/com.example.app=com.example.app.desktop\n\n[Added Associations]\nx-scheme-handler/com.example.app=com.example.app.desktop;\n"
    );
  }

  #[test]
  fn with_default_replaces_the_default_and_keeps_other_entries() {
    let existing = "[Default Applications]\ntext/html=firefox.desktop\nx-scheme-handler/com.example.app=old.desktop\n\n[Added Associations]\nx-scheme-handler/com.example.app=old.desktop;\n";
    let text = with_default(existing, "x-scheme-handler/com.example.app", "com.example.app.desktop");
    assert_eq!(
      text,
      "[Default Applications]\ntext/html=firefox.desktop\nx-scheme-handler/com.example.app=com.example.app.desktop\n\n[Added Associations]\nx-scheme-handler/com.example.app=old.desktop;com.example.app.desktop;\n"
    );
  }

  #[test]
  fn with_default_adds_a_missing_section_after_existing_ones() {
    let existing = "[Removed Associations]\ntext/plain=foo.desktop\n";
    let text = with_default(existing, "x-scheme-handler/com.example.app", "com.example.app.desktop");
    assert_eq!(
      text,
      "[Removed Associations]\ntext/plain=foo.desktop\n\n[Default Applications]\nx-scheme-handler/com.example.app=com.example.app.desktop\n\n[Added Associations]\nx-scheme-handler/com.example.app=com.example.app.desktop;\n"
    );
  }

  #[test]
  fn with_default_is_idempotent() {
    let once = with_default("", "x-scheme-handler/com.example.app", "com.example.app.desktop");
    let twice = with_default(&once, "x-scheme-handler/com.example.app", "com.example.app.desktop");
    assert_eq!(once, twice);
  }
}
