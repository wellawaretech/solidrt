//! Engine-free path core.
//!
//! The scripting-engine-independent half of `flux:path`: lexical join, the
//! resolve-within-a-trusted-base containment check, the three lexical
//! splits of a path (`basename`, `dirname`, `extname`), the path from one
//! place to another (`relative`) and glob matching (`matches_glob`). It names no scripting-engine
//! types; the marshalling layer (flux `forge_plugins/path.rs`) adapts JS args
//! and turns `None` into JS `null`.
//!
//! Glob patterns are the `glob` crate's: `*` any run of characters inside
//! one segment, `**` as a whole segment any number of segments, `?` one
//! character, `[a-z]` one of a set and `[!a-z]` one outside it. A character
//! the language reads as syntax is written as a set of one, `[*]`. There
//! are no `{a,b}` groups and no escape character.

use std::path::{Component, PathBuf, MAIN_SEPARATOR_STR};

use path_clean::PathClean;

/// Join and normalize `segments` lexically. Empty segments are skipped so a stray
/// "" cannot turn a relative join absolute; an empty result yields ".".
pub fn join(segments: &[String]) -> String {
  let joined = segments.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(MAIN_SEPARATOR_STR);
  if joined.is_empty() {
    return ".".to_string();
  }
  PathBuf::from(joined).clean().to_string_lossy().into_owned()
}

/// Resolve `path` against the trusted `base`, returning the absolute result only
/// if it stays inside `base`; otherwise `None`. Fusing normalization and
/// containment means a caller cannot resolve an untrusted path and forget to
/// check it did not escape. An absolute or `..`-laden `path` that climbs above
/// `base` fails the check and yields `None`.
pub fn resolve_within(base: &str, path: &str) -> Option<String> {
  let mut root = PathBuf::from(base);
  if root.is_relative() {
    if let Ok(cwd) = std::env::current_dir() {
      root = cwd.join(root);
    }
  }
  let root = root.clean();

  let joined = root.join(path).clean();

  // Component-wise prefix check (a path starts with itself), so a sibling like
  // `<root>-secret` is correctly rejected without a manual trailing-separator
  // dance.
  if joined.starts_with(&root) {
    Some(joined.to_string_lossy().into_owned())
  } else {
    None
  }
}

/// Whether `c` separates path components on this platform: `/` everywhere,
/// and `\` too on Windows.
fn is_separator(c: char) -> bool {
  c == '/' || (cfg!(windows) && c == '\\')
}

/// `path` without its trailing separators; a path of separators only keeps
/// its first, the root.
fn trim_trailing_separators(path: &str) -> &str {
  let trimmed = path.trim_end_matches(is_separator);
  if trimmed.is_empty() && !path.is_empty() {
    &path[..1]
  } else {
    trimmed
  }
}

/// The last component of `path`, trailing separators ignored; with `ext`,
/// that suffix is cut off unless it is the whole name. Lexical, Node's
/// `path.basename`: "/a/b.txt" is "b.txt", "/a/b/" is "b", the root is "".
pub fn basename(path: &str, ext: Option<&str>) -> String {
  let trimmed = trim_trailing_separators(path);
  let name = match trimmed.rfind(is_separator) {
    Some(at) => &trimmed[at + 1..],
    None => trimmed,
  };
  match ext {
    Some(ext) if !ext.is_empty() && name.len() > ext.len() && name.ends_with(ext) => {
      name[..name.len() - ext.len()].to_string()
    }
    _ => name.to_string(),
  }
}

/// `path` without its last component. Lexical, Node's `path.dirname`:
/// "/a/b.txt" is "/a", "b.txt" is ".", "/b" and the root are "/".
pub fn dirname(path: &str) -> String {
  let trimmed = trim_trailing_separators(path);
  match trimmed.rfind(is_separator) {
    None => ".".to_string(),
    Some(at) => {
      let parent = trim_trailing_separators(&trimmed[..at + 1]);
      parent.to_string()
    }
  }
}

/// The extension of `path`'s last component, from its last dot on, or ""
/// when it has none. Lexical, Node's `path.extname`: "a.tar.gz" is ".gz",
/// "a." is ".", and a leading dot names a hidden file, not an extension
/// (".bashrc" is "").
pub fn extname(path: &str) -> String {
  let name = basename(path, None);
  match name.rfind('.') {
    Some(at) if at > 0 && name != ".." => name[at..].to_string(),
    _ => String::new(),
  }
}

/// `path` as an absolute, normalized path: one that is relative stands
/// against the process cwd.
fn absolute(path: &str) -> PathBuf {
  let mut path = PathBuf::from(path);
  if path.is_relative() {
    if let Ok(cwd) = std::env::current_dir() {
      path = cwd.join(path);
    }
  }
  path.clean()
}

/// The path that leads from `from` to `to`, each standing against the
/// process cwd when relative. Lexical, Node's `path.relative`: from "/a/b"
/// to "/a/c/d.txt" is "../c/d.txt", a place to itself is "". Two paths with
/// nothing in common (another drive on Windows) give `to` as it resolved.
pub fn relative(from: &str, to: &str) -> String {
  let from = absolute(from);
  let to = absolute(to);
  let from_parts: Vec<Component> = from.components().collect();
  let to_parts: Vec<Component> = to.components().collect();
  let shared = from_parts.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();
  if shared == 0 {
    return to.to_string_lossy().into_owned();
  }
  let mut out = PathBuf::new();
  for _ in shared..from_parts.len() {
    out.push("..");
  }
  for part in &to_parts[shared..] {
    out.push(part);
  }
  out.to_string_lossy().into_owned()
}

/// How a glob pattern is matched: on the text as given, a wildcard never
/// standing for a separator, a leading dot an ordinary character.
pub(crate) const GLOB_OPTIONS: glob::MatchOptions =
  glob::MatchOptions { case_sensitive: true, require_literal_separator: true, require_literal_leading_dot: false };

/// The glob `pattern` compiled, or what is wrong with it and where.
pub(crate) fn glob_pattern(pattern: &str) -> Result<glob::Pattern, String> {
  glob::Pattern::new(pattern).map_err(|e| format!("the pattern {pattern:?} is malformed at character {}: {}", e.pos, e.msg))
}

/// Whether the glob `pattern` is well formed; what is wrong with it and
/// where, when it is not.
pub fn check_glob(pattern: &str) -> Result<(), String> {
  glob_pattern(pattern).map(|_| ())
}

/// Whether `path` matches the glob `pattern`, as a whole (the pattern
/// language is in the module docs). Lexical: nothing is read from disk and
/// nothing normalized. Errs on a malformed pattern (an unclosed set, a
/// `**` that is not a whole segment).
pub fn matches_glob(path: &str, pattern: &str) -> Result<bool, String> {
  Ok(glob_pattern(pattern)?.matches_with(path, GLOB_OPTIONS))
}
