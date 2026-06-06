#![cfg(all(unix, feature = "compile"))]

mod common;

use common::run_source;

// flux:path is a lexical path module: resolveWithin fuses normalization with a
// containment check, join concatenates and normalizes segments. Both are pure
// string operations (no filesystem access), so these tests assert on output
// for fixed inputs rather than touching disk. Absolute-path cases assume unix
// separators, hence the unix gate.

async fn eval(expr: &str) -> String {
  let code = format!(
    r#"
    import {{ resolveWithin, join }} from "flux:path";
    console.log(String({expr}));
    "#
  );
  let out = run_source(&code).await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  out.log()
}

#[tokio::test]
async fn resolve_within_returns_contained_path() {
  assert_eq!(eval(r#"resolveWithin("/srv/www", "index.html")"#).await, "/srv/www/index.html");
}

#[tokio::test]
async fn resolve_within_normalizes_interior_dot_dot() {
  // A `..` that stays inside the root is allowed and normalized away.
  assert_eq!(eval(r#"resolveWithin("/srv/www", "a/../b.html")"#).await, "/srv/www/b.html");
}

// A rejected resolve returns an explicit JS `null`, matching the documented
// `string | null` contract.

#[tokio::test]
async fn resolve_within_rejects_escape_via_dot_dot() {
  assert_eq!(eval(r#"resolveWithin("/srv/www", "../secret")"#).await, "null");
}

#[tokio::test]
async fn resolve_within_rejects_absolute_path() {
  assert_eq!(eval(r#"resolveWithin("/srv/www", "/etc/passwd")"#).await, "null");
}

#[tokio::test]
async fn resolve_within_rejects_sibling_prefix() {
  // Component-wise containment: `<root>-secret` shares a string prefix with the
  // root but is not inside it, so it must be rejected.
  assert_eq!(eval(r#"resolveWithin("/srv/www", "../www-secret")"#).await, "null");
}

#[tokio::test]
async fn join_concatenates_segments() {
  assert_eq!(eval(r#"join("assets", "img", "logo.png")"#).await, "assets/img/logo.png");
}

#[tokio::test]
async fn join_normalizes_dot_dot() {
  assert_eq!(eval(r#"join("a/b", "../c")"#).await, "a/c");
  assert_eq!(eval(r#"join("/foo", "..", "bar")"#).await, "/bar");
}

#[tokio::test]
async fn join_skips_empty_segments() {
  // An empty segment must not introduce a separator that turns the join
  // absolute; the result stays relative.
  assert_eq!(eval(r#"join("foo", "", "bar")"#).await, "foo/bar");
  assert_eq!(eval(r#"join("", "foo")"#).await, "foo");
}

#[tokio::test]
async fn join_of_nothing_is_dot() {
  assert_eq!(eval(r#"join()"#).await, ".");
}
