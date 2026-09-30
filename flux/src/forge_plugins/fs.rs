use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Exception, Function, Object};
use std::future::Future;

use super::{dir, file};
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{string_opt, with_in_flight, OptArg};
use forge::fs;

// `realpath(path)`: the canonical absolute path, resolved by the OS. A plain
// forward to forge::fs; the result is a string, so nothing to encode.
fn realpath<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Promised<impl Future<Output = JsResult<String>>>> {
  Ok(with_in_flight(&ctx, "fs", async move { fs::realpath(&path).await }))
}

// `rename(from, to)`: move a file or directory. A plain forward to
// forge::fs; both arguments are strings and the result is void, so nothing to
// encode.
fn rename<'js>(ctx: Ctx<'js>, from: String, to: String) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
  Ok(with_in_flight(&ctx, "fs", async move { fs::rename(&from, &to).await }))
}

// `glob(pattern, { cwd? })`: the files a glob pattern matches. A malformed
// pattern is the caller's mistake and throws at the call; the scan itself
// is a forward to forge::fs and resolves to the list of paths.
fn glob<'js>(
  ctx: Ctx<'js>,
  pattern: String,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Vec<String>>>>> {
  let cwd = match &opts.0 {
    Some(o) => string_opt(&ctx, o, "cwd", "glob")?,
    None => None,
  };
  forge::path::check_glob(&pattern).map_err(|e| Exception::throw_message(&ctx, &format!("glob: {e}")))?;
  Ok(with_in_flight(&ctx, "fs", async move { fs::glob(&pattern, cwd.as_deref()).await }))
}

pub struct FsModule;

impl ModuleDef for FsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("file")?;
    decl.declare("dir")?;
    decl.declare("realpath")?;
    decl.declare("rename")?;
    decl.declare("glob")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let _ = ctx.store_userdata(dir::InstalledWatches::default());
    exports.export("file", file::file_fn(ctx))?;
    exports.export("dir", dir::dir_fn(ctx))?;
    exports.export("realpath", Function::new(ctx.clone(), realpath)?)?;
    exports.export("rename", Function::new(ctx.clone(), rename)?)?;
    exports.export("glob", Function::new(ctx.clone(), glob)?)?;
    Ok(())
  }
}
