use crate::links::own_link;

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

#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
mod freedesktop {
  use crate::links::freedesktop::{desktop_entry, exec_quote, with_default};
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
