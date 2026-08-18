use rquickjs::function::Rest;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, IntoJs, Value};

use forge::path;

// Marshalling for `flux:path`: adapt JS args to the engine-free `forge::path`
// functions, and turn a containment failure (`None`) into JS `null`.

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

fn join(segments: Rest<String>) -> String {
  path::join(&segments.0)
}

// Returns the resolved absolute path, or an explicit JS `null` (not `undefined`)
// to match the documented `string | null` contract when `path` escapes `base`.
fn resolve_within<'js>(ctx: Ctx<'js>, base: String, path: String) -> rquickjs::Result<Value<'js>> {
  match path::resolve_within(&base, &path) {
    Some(resolved) => resolved.into_js(&ctx),
    None => Ok(Value::new_null(ctx)),
  }
}
