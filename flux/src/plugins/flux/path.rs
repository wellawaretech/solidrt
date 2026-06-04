use path_clean::PathClean;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function};
use std::path::PathBuf;

pub struct PathModule;

impl ModuleDef for PathModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("resolveWithin")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("resolveWithin", Function::new(ctx.clone(), resolve_within)?)?;
    Ok(())
  }
}

// Resolves `path` against the trusted `base` and returns the absolute result
// only if it stays inside `base`; otherwise returns null. This fuses
// normalization and containment so a caller cannot resolve an untrusted path
// and forget to verify it did not escape - the unsafe outcome is unrepresentable
// rather than a check that is easy to omit. An absolute or `..`-laden `path`
// that climbs above `base` simply fails the containment check and yields null.
// For a plain trusted join, pass the root as `base`.
fn resolve_within(base: String, path: String) -> Option<String> {
  let mut root = PathBuf::from(&base);
  if root.is_relative() {
    if let Ok(cwd) = std::env::current_dir() {
      root = cwd.join(root);
    }
  }
  let root = root.clean();

  let joined = root.join(&path).clean();

  // Component-wise prefix check (a path starts with itself), so a sibling like
  // `<root>-secret` is correctly rejected without a manual trailing-separator
  // dance.
  if joined.starts_with(&root) {
    Some(joined.to_string_lossy().into_owned())
  } else {
    None
  }
}