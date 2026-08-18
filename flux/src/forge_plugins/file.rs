use rquickjs::{function::MutFn, promise::Promised, Ctx, Exception, Function, Object, TypedArray, Value};
use std::rc::Rc;

use crate::pending::PendingOps;
use crate::plugins::marshal::with_pending;
use crate::plugins::seekable::SeekableSource;
use crate::standards_plugins::body::{attach_body, JsBytes};
use crate::plugins::value::Neutral;
use forge::fs;

// Marshalling for the `file()` reference: forward to the engine-free
// `forge::fs` disk operations and encode their results back to JS.

/// Decode a write/append payload: a string (UTF-8 bytes) or a Uint8Array.
fn data_bytes<'js, 'v>(ctx: &Ctx<'js>, data: &Value<'v>, what: &str) -> rquickjs::Result<Vec<u8>> {
  if let Some(s) = data.as_string() {
    Ok(s.to_string()?.into_bytes())
  } else if let Ok(ta) = TypedArray::<u8>::from_value(data.clone()) {
    Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
  } else {
    Err(Exception::throw_message(ctx, &format!("{what}: data must be string or Uint8Array")))
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
        Ok(with_pending(&ctx, async move { fs::stat(&path).await.map(|s| Neutral(s.into())) }))
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
        let bytes = data_bytes(&ctx, &data, "write")?;
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::write(&path, &bytes).await }))
      }
    }),
  )
  .expect("create write function");
  obj.set("write", write_fn)?;

  let append_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>, data: Value<'_>| -> rquickjs::Result<Promised<_>> {
        let bytes = data_bytes(&ctx, &data, "append")?;
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::append(&path, &bytes).await }))
      }
    }),
  )
  .expect("create append function");
  obj.set("append", append_fn)?;

  // Carry a native seekable source so a consumer that needs sync, seekable bytes
  // off the JS thread (audio streaming) can open the file on demand. The opener
  // resolves through forge::fs, so a packed asset hands out a range-read window
  // into the exe instead of a plain disk file.
  let path_for_open = path.clone();
  SeekableSource::attach(&ctx, &obj, Rc::new(move || fs::open_seekable(&path_for_open)))?;

  Ok(obj)
}

pub(crate) fn file_fn<'js>(ctx: &Ctx<'js>) -> Function<'js> {
  Function::new(ctx.clone(), build_file).expect("create file function")
}
