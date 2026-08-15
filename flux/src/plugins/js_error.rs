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
