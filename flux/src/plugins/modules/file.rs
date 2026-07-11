use rquickjs::{function::MutFn, promise::Promised, Ctx, Exception, Function, IntoJs, Object, TypedArray, Value};
use std::rc::Rc;

use crate::pending::PendingOps;
use crate::plugins::marshal::with_pending;
use crate::plugins::seekable::SeekableSource;
use crate::plugins::standards::body::{attach_body, JsBytes};
use forge::{fs, SeekableReader};

// Marshalling for the `file()` reference: forward to the engine-free
// `forge::fs` disk operations and encode their results back to JS.

// Marshalling newtype over the engine-free `forge::fs::StatInfo`, so its
// `IntoJs` stays in this crate once forge is split out (a foreign `IntoJs` on a
// foreign type would otherwise trip the orphan rule). The `stat` call site
// `.map(JsStatInfo)`s the bare forge result.
struct JsStatInfo(fs::StatInfo);

impl<'js> IntoJs<'js> for JsStatInfo {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("size", self.0.size)?;
    obj.set("type", self.0.file_type)?;
    if let Some(m) = self.0.mtime_ms {
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
        Ok(with_pending(&ctx, async move { fs::stat(&path).await.map(JsStatInfo) }))
      }
    }),
  )
  .expect("create stat function");
  obj.set("stat", stat_fn)?;

  // Ranged read: `read(offset, length)` resolves to a Uint8Array of exactly
  // `length` bytes (a range past end-of-file rejects; clamp against stat size).
  let read_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>, offset: f64, length: f64| -> rquickjs::Result<Promised<_>> {
        if offset < 0.0 || offset.fract() != 0.0 || length < 0.0 || length.fract() != 0.0 {
          return Err(Exception::throw_message(&ctx, "read: offset and length must be non-negative integers"));
        }
        let (offset, length) = (offset as u64, length as u64);
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::read_range(&path, offset, length).await.map(JsBytes) }))
      }
    }),
  )
  .expect("create read function");
  obj.set("read", read_fn)?;

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

  // Carry a native seekable source so a consumer that needs sync, seekable bytes
  // off the JS thread (audio streaming) can open the file on demand. Local disk
  // here; the dev-server proxy attaches a range-backed opener instead.
  let path_for_open = path.clone();
  SeekableSource::attach(
    &ctx,
    &obj,
    Rc::new(move || fs::open_seekable(&path_for_open).map(|f| Box::new(f) as SeekableReader)),
  )?;

  Ok(obj)
}

pub(crate) fn file_fn<'js>(ctx: &Ctx<'js>) -> Function<'js> {
  Function::new(ctx.clone(), build_file).expect("create file function")
}
