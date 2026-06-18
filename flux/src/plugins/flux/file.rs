use rquickjs::{function::MutFn, promise::Promised, Ctx, Exception, Function, IntoJs, Object, TypedArray, Value};
use std::rc::Rc;

use crate::forge::fs;
use crate::pending::PendingOps;
use crate::plugins::body::attach_body;
use crate::plugins::marshal::with_pending;

// Marshalling for the `file()` reference: forward to the engine-free
// `forge::fs` disk operations and encode their results back to JS.

impl<'js> IntoJs<'js> for fs::StatInfo {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("size", self.size)?;
    obj.set("type", self.file_type)?;
    if let Some(m) = self.mtime_ms {
      obj.set("mtime", m)?;
    }
    Ok(obj.into_value())
  }
}

fn build_file<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Object<'js>> {
  let path = Rc::new(path);
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path.as_ref().clone())?;

  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();

  // Body methods read from disk on each call (not consume-once).
  let path_for_body = path.clone();
  let pending_for_body = pending.clone();
  attach_body(
    &ctx,
    &obj,
    move || {
      let path = path_for_body.clone();
      let pending = pending_for_body.clone();
      async move {
        pending.hold();
        let r = fs::read(&path).await;
        pending.release();
        r
      }
    },
    false,
  )?;

  let exists_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { Ok::<bool, String>(fs::file_exists(&path).await) }))
      }
    }),
  )
  .expect("create exists function");
  obj.set("exists", exists_fn)?;

  let stat_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::stat(&path).await }))
      }
    }),
  )
  .expect("create stat function");
  obj.set("stat", stat_fn)?;

  let write_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>, data: Value<'_>| -> rquickjs::Result<Promised<_>> {
        let bytes = if let Some(s) = data.as_string() {
          s.to_string()?.into_bytes()
        } else if let Ok(ta) = TypedArray::<u8>::from_value(data.clone()) {
          ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default()
        } else {
          return Err(Exception::throw_message(&ctx, "write: data must be string or Uint8Array"));
        };
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::write(&path, &bytes).await }))
      }
    }),
  )
  .expect("create write function");
  obj.set("write", write_fn)?;

  Ok(obj)
}

pub(crate) fn file_fn<'js>(ctx: &Ctx<'js>) -> Function<'js> {
  Function::new(ctx.clone(), build_file).expect("create file function")
}