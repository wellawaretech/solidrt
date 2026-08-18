use rquickjs::{function::MutFn, promise::Promised, Ctx, Function, Object};
use std::rc::Rc;

use crate::plugins::marshal::with_pending;
use crate::plugins::value::Neutral;
use forge::fs;
use forge::Value;

// Marshalling for the `dir()` reference: forward to the engine-free
// `forge::fs` directory operations and encode their results back to JS.

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
        Ok(with_pending(&ctx, async move { fs::read_dir(&path).await.map(|entries| Neutral(Value::list(entries))) }))
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

  let create_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::create_dir(&path).await }))
      }
    }),
  )
  .expect("create create function");
  obj.set("create", create_fn)?;

  Ok(obj)
}

pub(crate) fn dir_fn<'js>(ctx: &Ctx<'js>) -> Function<'js> {
  Function::new(ctx.clone(), build_dir).expect("create dir function")
}
