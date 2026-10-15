use crate::dialog::{name_of_path, sdl_pattern, DialogFilter};

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
