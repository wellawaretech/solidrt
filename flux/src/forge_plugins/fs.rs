use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Function};
use std::future::Future;

use super::{dir, file};
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::with_pending;
use forge::fs;

// `realpath(path)`: the canonical absolute path, resolved by the OS. A plain
// forward to forge::fs; the result is a string, so nothing to encode.
fn realpath<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Promised<impl Future<Output = JsResult<String>>>> {
  Ok(with_pending(&ctx, async move { fs::realpath(&path).await }))
}

// `rename(from, to)`: move a file or directory. A plain forward to
// forge::fs; both arguments are strings and the result is void, so nothing to
// encode.
fn rename<'js>(ctx: Ctx<'js>, from: String, to: String) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
  Ok(with_pending(&ctx, async move { fs::rename(&from, &to).await }))
}

pub struct FsModule;

impl ModuleDef for FsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("file")?;
    decl.declare("dir")?;
    decl.declare("realpath")?;
    decl.declare("rename")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let _ = ctx.store_userdata(dir::InstalledWatches::default());
    exports.export("file", file::file_fn(ctx))?;
    exports.export("dir", dir::dir_fn(ctx))?;
    exports.export("realpath", Function::new(ctx.clone(), realpath)?)?;
    exports.export("rename", Function::new(ctx.clone(), rename)?)?;
    Ok(())
  }
}
