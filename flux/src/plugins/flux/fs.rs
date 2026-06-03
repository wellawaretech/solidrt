use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::Ctx;

use super::{dir, file};

pub struct FsModule;

impl ModuleDef for FsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("file")?;
    decl.declare("dir")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("file", file::file_fn(ctx))?;
    exports.export("dir", dir::dir_fn(ctx))?;
    Ok(())
  }
}