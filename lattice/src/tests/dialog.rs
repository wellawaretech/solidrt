use alloy::{DialogError, DialogFilter, DialogKind, DialogRequest, DialogStatus, Picked};

use crate::dialog::{parse_answer, request_json, status_json};

#[test]
fn files_answer_names_each_by_its_file_name_unless_named() {
  let json = serde_json::json!({ "files": ["docs/a.txt", { "path": "x.bin", "name": "Photo.bin" }] });
  let picked = parse_answer(&json).expect("parses");
  let pairs: Vec<(&str, &str)> = picked.iter().map(|p| (p.location.as_str(), p.name.as_str())).collect();
  assert_eq!(pairs, vec![("docs/a.txt", "a.txt"), ("x.bin", "Photo.bin")]);
}

#[test]
fn directory_answer_is_one_entry_and_cancel_is_none() {
  let folder = parse_answer(&serde_json::json!({ "directory": "/home/u/docs" })).expect("parses");
  assert_eq!(folder.len(), 1);
  assert_eq!(folder[0].name, "docs");
  assert!(parse_answer(&serde_json::json!({ "cancel": true })).expect("parses").is_empty());
}

#[test]
fn other_shapes_are_refused() {
  assert!(parse_answer(&serde_json::json!({ "paths": ["a"] })).is_err());
  assert!(parse_answer(&serde_json::json!({ "files": [1] })).is_err());
  assert!(parse_answer(&serde_json::json!("a.txt")).is_err());
}

#[test]
fn a_request_is_its_kind_and_options_in_the_pickers_words() {
  let filters = vec![DialogFilter { name: "Text".to_string(), extensions: vec!["txt".to_string()] }];
  let open = DialogRequest { kind: DialogKind::OpenFile { multiple: true }, filters: filters.clone(), start_in: None };
  assert_eq!(
    request_json(&open),
    serde_json::json!({ "kind": "openFiles", "multiple": true, "types": [{ "description": "Text", "extensions": ["txt"] }], "startIn": null })
  );
  let save = DialogRequest {
    kind: DialogKind::SaveFile { suggested_name: Some("report.csv".to_string()) },
    filters: Vec::new(),
    start_in: Some("out".to_string()),
  };
  assert_eq!(
    request_json(&save),
    serde_json::json!({ "kind": "saveFile", "suggestedName": "report.csv", "types": [], "startIn": "out" })
  );
  let folder = DialogRequest { kind: DialogKind::OpenDirectory, filters: Vec::new(), start_in: Some(".".to_string()) };
  assert_eq!(request_json(&folder), serde_json::json!({ "kind": "openDirectory", "startIn": "." }));
}

#[test]
fn the_status_carries_the_last_requests_answer_once_it_has_one() {
  let folder = DialogRequest { kind: DialogKind::OpenDirectory, filters: Vec::new(), start_in: None };
  let waiting =
    DialogStatus { available: true, pending: Some(folder.clone()), last: Some((folder.clone(), None)), queued: 2 };
  assert_eq!(
    status_json(&waiting),
    serde_json::json!({
      "pending": { "kind": "openDirectory", "startIn": null },
      "last": { "kind": "openDirectory", "startIn": null },
      "queued": 2,
      "available": true,
    })
  );
  let picked = Picked { location: "docs".to_string(), name: "docs".to_string() };
  let answered =
    DialogStatus { available: false, pending: None, last: Some((folder.clone(), Some(Ok(vec![picked])))), queued: 0 };
  assert_eq!(
    status_json(&answered)["last"]["answer"],
    serde_json::json!({ "picked": [{ "location": "docs", "name": "docs" }] })
  );
  let cancelled = DialogStatus { last: Some((folder.clone(), Some(Ok(Vec::new())))), ..DialogStatus::default() };
  assert_eq!(status_json(&cancelled)["last"]["answer"], serde_json::json!({ "cancelled": true }));
  let failed = DialogStatus { last: Some((folder, Some(Err(DialogError::Unavailable)))), ..DialogStatus::default() };
  assert_eq!(
    status_json(&failed)["last"]["answer"],
    serde_json::json!({ "failed": "no file dialog on this platform" })
  );
  assert_eq!(status_json(&DialogStatus::default())["pending"], serde_json::Value::Null);
}
