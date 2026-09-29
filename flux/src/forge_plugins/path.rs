use rquickjs::function::Rest;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, IntoJs, Value};

use crate::plugins::marshal::OptArg;
use forge::path;

// Marshalling for `flux:path`: adapt JS args to the engine-free `forge::path`
// functions, and turn a containment failure (`None`) into JS `null`.
// basename/dirname/extname are Node's, lexical like the rest.

pub struct PathModule;

impl ModuleDef for PathModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("resolveWithin")?;
    decl.declare("join")?;
    decl.declare("basename")?;
    decl.declare("dirname")?;
    decl.declare("extname")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("resolveWithin", Function::new(ctx.clone(), resolve_within)?)?;
    exports.export("join", Function::new(ctx.clone(), join)?)?;
    exports.export("basename", Function::new(ctx.clone(), basename)?)?;
    exports.export("dirname", Function::new(ctx.clone(), dirname)?)?;
    exports.export("extname", Function::new(ctx.clone(), extname)?)?;
    Ok(())
  }
}

fn join(segments: Rest<String>) -> String {
  path::join(&segments.0)
}

fn basename(path: String, ext: OptArg<String>) -> String {
  path::basename(&path, ext.0.as_deref())
}

fn dirname(path: String) -> String {
  path::dirname(&path)
}

fn extname(path: String) -> String {
  path::extname(&path)
}

// Returns the resolved absolute path, or an explicit JS `null` (not `undefined`)
// to match the documented `string | null` contract when `path` escapes `base`.
fn resolve_within<'js>(ctx: Ctx<'js>, base: String, path: String) -> rquickjs::Result<Value<'js>> {
  match path::resolve_within(&base, &path) {
    Some(resolved) => resolved.into_js(&ctx),
    None => Ok(Value::new_null(ctx)),
  }
}
