use rquickjs::{function::MutFn, promise::Promised, Array, Ctx, Function, IntoJs, Object, Value};
use std::rc::Rc;

use crate::plugins::marshal::with_pending;
use forge::fs;

// Marshalling for the `dir()` reference: forward to the engine-free
// `forge::fs` directory operations and encode their results back to JS.

// A directory listing: (name, type) pairs from forge::fs::read_dir.
struct DirEntries(Vec<(String, &'static str)>);

impl<'js> IntoJs<'js> for DirEntries {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (i, (name, kind)) in self.0.into_iter().enumerate() {
      let entry = Object::new(ctx.clone())?;
      entry.set("name", name)?;
      entry.set("type", kind)?;
      arr.set(i, entry)?;
    }
    Ok(arr.into_value())
  }
}

fn build_dir<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Object<'js>> {
  let path = Rc::new(path);
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path.as_ref().clone())?;

  let entries_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::read_dir(&path).await.map(DirEntries) }))
      }
    }),
  )
  .expect("create entries function");
  obj.set("entries", entries_fn)?;

  let exists_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { Ok::<bool, String>(fs::dir_exists(&path).await) }))
      }
    }),
  )
  .expect("create exists function");
  obj.set("exists", exists_fn)?;

  Ok(obj)
}

pub(crate) fn dir_fn<'js>(ctx: &Ctx<'js>) -> Function<'js> {
  Function::new(ctx.clone(), build_dir).expect("create dir function")
}
