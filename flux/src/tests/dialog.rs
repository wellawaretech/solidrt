use crate::alloy_plugins::dialog::parse_answer;

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
