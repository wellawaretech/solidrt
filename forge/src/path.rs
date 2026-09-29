//! Engine-free path core.
//!
//! The scripting-engine-independent half of `flux:path`: lexical join, the
//! resolve-within-a-trusted-base containment check, and the three lexical
//! splits of a path (`basename`, `dirname`, `extname`). It names no scripting-engine
//! types; the marshalling layer (flux `forge_plugins/path.rs`) adapts JS args
//! and turns `None` into JS `null`.

use std::path::{PathBuf, MAIN_SEPARATOR_STR};

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
