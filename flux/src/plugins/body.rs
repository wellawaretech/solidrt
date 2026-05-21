use rquickjs::{
  function::MutFn, promise::Promised, Ctx, Function, IntoJs, Object, TypedArray, Value,
};
use std::cell::Cell;
use std::future::Future;
use std::io;
use std::rc::Rc;

pub(crate) struct JsBytes(pub Vec<u8>);

impl<'js> IntoJs<'js> for JsBytes {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    TypedArray::<u8>::new(ctx.clone(), self.0).map(|ta| ta.into_value())
  }
}

pub(crate) struct JsonValue(pub String);

impl<'js> IntoJs<'js> for JsonValue {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    ctx.json_parse(self.0)
  }
}

pub(crate) fn throw_consumed(ctx: &Ctx<'_>) -> rquickjs::Error {
  ctx.throw(
    rquickjs::String::from_str(ctx.clone(), "Body already consumed")
      .expect("create error string")
      .into(),
  )
}

fn utf8_err(e: std::string::FromUtf8Error) -> rquickjs::Error {
  rquickjs::Error::Io(io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Attach text(), bytes(), json() methods to obj.
///
/// fetch_bytes is invoked lazily on each method call to obtain the body bytes.
/// If consume_once is true, calling any of the three methods more than once
/// throws "Body already consumed" (web fetch semantics). If false, methods can
/// be called repeatedly (file-like semantics).
pub(crate) fn attach_body<'js, F, Fut>(
  ctx: &Ctx<'js>,
  obj: &Object<'js>,
  fetch_bytes: F,
  consume_once: bool,
) -> rquickjs::Result<()>
where
  F: Fn() -> Fut + Clone + 'static,
  Fut: Future<Output = rquickjs::Result<Vec<u8>>> + 'static,
{
  let consumed = Rc::new(Cell::new(false));

  let text_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let consumed = consumed.clone();
      let fetch_bytes = fetch_bytes.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        if consume_once && consumed.get() {
          return Err(throw_consumed(&ctx));
        }
        consumed.set(true);
        let fetch = fetch_bytes.clone();
        Ok(Promised(async move {
          let bytes = fetch().await?;
          String::from_utf8(bytes).map_err(utf8_err)
        }))
      }
    }),
  )
  .expect("create text function");

  let bytes_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let consumed = consumed.clone();
      let fetch_bytes = fetch_bytes.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        if consume_once && consumed.get() {
          return Err(throw_consumed(&ctx));
        }
        consumed.set(true);
        let fetch = fetch_bytes.clone();
        Ok(Promised(async move {
          let bytes = fetch().await?;
          Ok::<JsBytes, rquickjs::Error>(JsBytes(bytes))
        }))
      }
    }),
  )
  .expect("create bytes function");

  let json_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let consumed = consumed.clone();
      let fetch_bytes = fetch_bytes.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        if consume_once && consumed.get() {
          return Err(throw_consumed(&ctx));
        }
        consumed.set(true);
        let fetch = fetch_bytes.clone();
        Ok(Promised(async move {
          let bytes = fetch().await?;
          let text = String::from_utf8(bytes).map_err(utf8_err)?;
          Ok::<JsonValue, rquickjs::Error>(JsonValue(text))
        }))
      }
    }),
  )
  .expect("create json function");

  obj.set("text", text_fn)?;
  obj.set("bytes", bytes_fn)?;
  obj.set("json", json_fn)?;

  Ok(())
}