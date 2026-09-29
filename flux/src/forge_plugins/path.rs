use rquickjs::function::Rest;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function, IntoJs, Value};

use crate::plugins::marshal::OptArg;
use forge::path;

// Marshalling for `flux:path`: adapt JS args to the engine-free `forge::path`
// functions, and turn a containment failure (`None`) into JS `null`.
// basename/dirname/extname/relative/matchesGlob are Node's, lexical like
// the rest; a malformed glob pattern throws.

pub struct PathModule;

impl ModuleDef for PathModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("resolveWithin")?;
    decl.declare("join")?;
    decl.declare("basename")?;
    decl.declare("dirname")?;
    decl.declare("extname")?;
    decl.declare("relative")?;
    decl.declare("matchesGlob")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("resolveWithin", Function::new(ctx.clone(), resolve_within)?)?;
    exports.export("join", Function::new(ctx.clone(), join)?)?;
    exports.export("basename", Function::new(ctx.clone(), basename)?)?;
    exports.export("dirname", Function::new(ctx.clone(), dirname)?)?;
    exports.export("extname", Function::new(ctx.clone(), extname)?)?;
    exports.export("relative", Function::new(ctx.clone(), relative)?)?;
    exports.export("matchesGlob", Function::new(ctx.clone(), matches_glob)?)?;
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

fn relative(from: String, to: String) -> String {
  path::relative(&from, &to)
}

fn matches_glob(ctx: Ctx<'_>, path: String, pattern: String) -> rquickjs::Result<bool> {
  path::matches_glob(&path, &pattern).map_err(|e| Exception::throw_message(&ctx, &format!("matchesGlob: {e}")))
}

// Returns the resolved absolute path, or an explicit JS `null` (not `undefined`)
// to match the documented `string | null` contract when `path` escapes `base`.
fn resolve_within<'js>(ctx: Ctx<'js>, base: String, path: String) -> rquickjs::Result<Value<'js>> {
  match path::resolve_within(&base, &path) {
    Some(resolved) => resolved.into_js(&ctx),
    None => Ok(Value::new_null(ctx)),
  }
}
