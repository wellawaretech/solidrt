use crate::folders::home_dir;

#[test]
fn home_dir_is_the_environment_home() {
  let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
  let expected = std::env::var(var).ok().filter(|v| !v.is_empty());
  assert_eq!(home_dir(), expected);
}
