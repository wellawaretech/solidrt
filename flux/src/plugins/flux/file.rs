use rquickjs::{function::MutFn, promise::Promised, Ctx, Function, IntoJs, Object, Value};
use std::rc::Rc;

use crate::pending::PendingOps;
use crate::plugins::body::attach_body;

struct StatResult {
  size: u64,
  file_type: &'static str,
  mtime_ms: Option<i64>,
}

impl<'js> IntoJs<'js> for StatResult {
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

fn file_type_str(ft: std::fs::FileType) -> &'static str {
  if ft.is_file() {
    "file"
  } else if ft.is_dir() {
    "directory"
  } else if ft.is_symlink() {
    "symlink"
  } else {
    "other"
  }
}

fn mtime_ms(meta: &std::fs::Metadata) -> Option<i64> {
  let mtime = meta.modified().ok()?;
  let dur = mtime
    .duration_since(std::time::SystemTime::UNIX_EPOCH)
    .ok()?;
  i64::try_from(dur.as_millis()).ok()
}

fn build_file<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Object<'js>> {
  let path = Rc::new(path);
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path.as_ref().clone())?;

  let pending = ctx
    .userdata::<PendingOps>()
    .expect("pending ops")
    .clone();

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
        let r = tokio::fs::read(&**path).await.map_err(rquickjs::Error::Io);
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
        let pending = ctx
          .userdata::<PendingOps>()
          .expect("pending ops")
          .clone();
        let path = path.clone();
        Ok(Promised(async move {
          pending.hold();
          let exists = tokio::fs::metadata(&**path)
            .await
            .map(|m| m.is_file())
            .unwrap_or(false);
          pending.release();
          Ok::<bool, rquickjs::Error>(exists)
        }))
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
        let pending = ctx
          .userdata::<PendingOps>()
          .expect("pending ops")
          .clone();
        let path = path.clone();
        Ok(Promised(async move {
          pending.hold();
          let r = tokio::fs::metadata(&**path).await;
          pending.release();
          let meta = r.map_err(rquickjs::Error::Io)?;
          Ok::<StatResult, rquickjs::Error>(StatResult {
            size: meta.len(),
            file_type: file_type_str(meta.file_type()),
            mtime_ms: mtime_ms(&meta),
          })
        }))
      }
    }),
  )
  .expect("create stat function");
  obj.set("stat", stat_fn)?;

  Ok(obj)
}

pub(crate) fn init_file<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  let file_fn =
    Function::new(ctx.clone(), build_file).expect("create Flux.file function");
  flux.set("file", file_fn).expect("set Flux.file");
}