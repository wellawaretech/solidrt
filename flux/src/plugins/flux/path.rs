use path_clean::PathClean;
use rquickjs::function::Rest;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, IntoJs, Value};
use std::path::{PathBuf, MAIN_SEPARATOR_STR};

pub struct PathModule;

impl ModuleDef for PathModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("resolveWithin")?;
    decl.declare("join")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("resolveWithin", Function::new(ctx.clone(), resolve_within)?)?;
    exports.export("join", Function::new(ctx.clone(), join)?)?;
    Ok(())
  }
}

// Joins and normalizes `segments` lexically. Empty segments are skipped so a
// stray "" cannot turn a relative join absolute; an empty result yields ".".
fn join(segments: Rest<String>) -> String {
  let joined = segments.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(MAIN_SEPARATOR_STR);
  if joined.is_empty() {
    return ".".to_string();
  }
  PathBuf::from(joined).clean().to_string_lossy().into_owned()
}

// Resolves `path` against the trusted `base`, returning the absolute result
// only if it stays inside `base`; otherwise null. Fusing normalization and
// containment means a caller cannot resolve an untrusted path and forget to
// check it did not escape. An absolute or `..`-laden `path` that climbs above
// `base` fails the check and yields null.
fn resolve_within<'js>(ctx: Ctx<'js>, base: String, path: String) -> rquickjs::Result<Value<'js>> {
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
  // dance. A rejection returns an explicit JS `null` (not `undefined`) to match
  // the documented `string | null` contract.
  if joined.starts_with(&root) {
    joined.to_string_lossy().into_owned().into_js(&ctx)
  } else {
    Ok(Value::new_null(ctx))
  }
}