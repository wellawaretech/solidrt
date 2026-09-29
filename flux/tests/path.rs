#![cfg(all(unix, feature = "compile"))]

mod common;

use common::run_source;

// flux:path is a lexical path module: resolveWithin fuses normalization with a
// containment check, join concatenates and normalizes segments,
// basename/dirname/extname split a path the way Node's do, relative leads
// from one path to another and matchesGlob matches a path against a glob
// pattern. All are pure
// string operations (no filesystem access), so these tests assert on output
// for fixed inputs rather than touching disk. Absolute-path cases assume unix
// separators, hence the unix gate.

async fn eval(expr: &str) -> String {
  let code = format!(
    r#"
    import {{ resolveWithin, join, basename, dirname, extname, relative, matchesGlob }} from "flux:path";
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

#[tokio::test]
async fn basename_dirname_extname_split_a_path() {
  assert_eq!(eval(r#"basename("/a/model.gltf")"#).await, "model.gltf");
  assert_eq!(eval(r#"basename("/a/model.gltf", ".gltf")"#).await, "model");
  assert_eq!(eval(r#"dirname("/a/model.gltf")"#).await, "/a");
  assert_eq!(eval(r#"dirname("model.gltf")"#).await, ".");
  assert_eq!(eval(r#"extname("/a/model.gltf")"#).await, ".gltf");
  assert_eq!(eval(r#"extname("model")"#).await, "");
}

// A wrapper forwards its own optional parameter as it got it, so an explicit
// undefined reads as "no ext".
#[tokio::test]
async fn basename_takes_an_explicit_undefined_ext() {
  assert_eq!(eval(r#"basename("/a/model.gltf", undefined)"#).await, "model.gltf");
}

#[tokio::test]
async fn relative_leads_from_one_path_to_another() {
  assert_eq!(eval(r#"relative("/a/b", "/a/c/d.txt")"#).await, "../c/d.txt");
  assert_eq!(eval(r#"relative("/a/b", "/a/b/c")"#).await, "c");
  assert_eq!(eval(r#"relative("/a/b", "/a/b").length"#).await, "0");
  // Against the cwd on both sides, so the cwd cancels out.
  assert_eq!(eval(r#"relative(".", "assets/a.png")"#).await, "assets/a.png");
}

#[tokio::test]
async fn matches_glob_matches_whole_paths() {
  assert_eq!(eval(r#"matchesGlob("assets/lion_head.png", "assets/lion_*.png")"#).await, "true");
  assert_eq!(eval(r#"matchesGlob("assets/sub/lion_head.png", "assets/lion_*.png")"#).await, "false");
  assert_eq!(eval(r#"matchesGlob("assets/sub/deep/a.png", "assets/**/*.png")"#).await, "true");
  assert_eq!(eval(r#"matchesGlob("assets/tile_7.png", "assets/tile_[0-9].png")"#).await, "true");
  assert_eq!(eval(r#"matchesGlob("assets/Tile_7.png", "assets/tile_?.png")"#).await, "false");
}

#[tokio::test]
async fn matches_glob_throws_on_a_malformed_pattern() {
  let thrown = eval(r#"(() => { try { return matchesGlob("ab", "a[b") } catch (e) { return e.message } })()"#).await;
  assert_eq!(thrown, "matchesGlob: the pattern \"a[b\" is malformed at character 1: invalid range pattern");
}
