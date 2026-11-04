use rquickjs::{Ctx, Exception, IntoJs, Value};

/// Wraps a fallible async result so its error becomes a clean JS `Error` (just
/// the message, no "IO Error:" prefix). The conversion happens in `into_js`,
/// which runs on the JS thread with `ctx` in hand; that lets `Promised` futures
/// report errors without capturing `Ctx` (the future's lifetime forbids holding
/// one across an await).
pub struct JsResult<T>(pub Result<T, String>);

impl<'js, T: IntoJs<'js>> IntoJs<'js> for JsResult<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self.0 {
      Ok(value) => value.into_js(ctx),
      Err(message) => Err(Exception::throw_message(ctx, &message)),
    }
  }
}

/// `JsResult` whose error is a `TypeError`: for a standard whose contract
/// names that class for its failures (the compression streams' corrupt,
/// cut-short and trailing-data errors).
pub struct JsTypeResult<T>(pub Result<T, String>);

impl<'js, T: IntoJs<'js>> IntoJs<'js> for JsTypeResult<T> {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    match self.0 {
      Ok(value) => value.into_js(ctx),
      Err(message) => Err(Exception::throw_type(ctx, &message)),
    }
  }
}

/// An `Error` with a DOMException-style `name`, for a rejection code can
/// branch on (`e.name === "NotSupportedError"`).
pub(crate) fn named_error<'js>(ctx: &Ctx<'js>, name: &str, message: &str) -> rquickjs::Result<Value<'js>> {
  let error = Exception::from_message(ctx.clone(), message)?;
  error.set("name", name)?;
  Ok(error.into_value())
}
