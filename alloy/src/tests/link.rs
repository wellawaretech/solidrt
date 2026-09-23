use crate::event::link_from_drop;

#[test]
fn custom_scheme_is_a_link() {
  assert_eq!(link_from_drop("myapp://settings/theme").as_deref(), Some("myapp://settings/theme"));
  assert_eq!(link_from_drop("com.example.app:/item/42").as_deref(), Some("com.example.app:/item/42"));
  assert_eq!(link_from_drop("https://example.com/x").as_deref(), Some("https://example.com/x"));
}

#[test]
fn file_paths_are_not_links() {
  assert_eq!(link_from_drop("/home/user/file.txt"), None);
  assert_eq!(link_from_drop("C:\\Users\\file.txt"), None);
  assert_eq!(link_from_drop("file.txt"), None);
  assert_eq!(link_from_drop(""), None);
}

#[test]
fn malformed_schemes_are_not_links() {
  assert_eq!(link_from_drop("1abc://x"), None);
  assert_eq!(link_from_drop("my app://x"), None);
  assert_eq!(link_from_drop(":no-scheme"), None);
}
