//! Engine-free path core.
//!
//! The scripting-engine-independent half of `flux:path`: lexical join and the
//! resolve-within-a-trusted-base containment check. It names no scripting-engine
//! types; the marshalling layer (`plugins/flux/path.rs`) adapts JS args and
//! turns `None` into JS `null`. Destined for the `forge` crate (see REDESIGN.md).

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