use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, JsLifetime, Object, Value};

use crate::plugins::body::{body_bytes, body_json, body_text, extract_body_value, BodyState, JsBytes, JsonValue};
use crate::plugins::headers::{headers_from_init, headers_from_pairs, Headers};

#[derive(JsLifetime)]
#[rquickjs::class(rename = "Request")]
pub struct Request<'js> {
  #[qjs(skip_trace)]
  pub(crate) body: BodyState,
  #[qjs(skip_trace)]
  pub(crate) method: String,
  #[qjs(skip_trace)]
  pub(crate) url: String,
  pub(crate) headers: Class<'js, Headers>,
}

impl<'js> Trace<'js> for Request<'js> {
  fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
    self.headers.trace(tracer);
  }
}

#[rquickjs::methods]
impl<'js> Request<'js> {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>, url: String, init: Opt<Object<'js>>) -> rquickjs::Result<Self> {
    let method: String = init
      .0
      .as_ref()
      .and_then(|o| o.get::<_, String>("method").ok())
      .unwrap_or_else(|| "GET".to_string())
      .to_uppercase();
    let body_bytes = match init.0.as_ref().and_then(|o| o.get::<_, Value>("body").ok()) {
      Some(v) => extract_body_value(&v, "Request")?,
      None => Vec::new(),
    };
    let headers_val = init.0.as_ref().and_then(|o| o.get::<_, Value>("headers").ok());
    let headers = headers_from_init(&ctx, headers_val.as_ref())?;
    Ok(Request { body: BodyState::new(body_bytes), method, url, headers })
  }

  #[qjs(get)]
  pub fn method(&self) -> String {
    self.method.clone()
  }

  #[qjs(get)]
  pub fn url(&self) -> String {
    self.url.clone()
  }

  #[qjs(get)]
  pub fn headers(&self) -> Class<'js, Headers> {
    self.headers.clone()
  }

  pub fn text(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<std::future::Ready<rquickjs::Result<String>>>> {
    let text = body_text(&self.body, &ctx)?;
    Ok(Promised(std::future::ready(Ok(text))))
  }

  pub fn bytes(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<std::future::Ready<rquickjs::Result<JsBytes>>>> {
    let bytes = body_bytes(&self.body, &ctx)?;
    Ok(Promised(std::future::ready(Ok(bytes))))
  }

  pub fn json(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<std::future::Ready<rquickjs::Result<JsonValue>>>> {
    let json = body_json(&self.body, &ctx)?;
    Ok(Promised(std::future::ready(Ok(json))))
  }
}

/// Build a Request directly from Rust state (used by serve.rs after reading the incoming body).
pub(crate) fn request_from_parts<'js>(
  ctx: &Ctx<'js>,
  method: String,
  url: String,
  body: Vec<u8>,
  headers: Vec<(String, String)>,
) -> rquickjs::Result<Class<'js, Request<'js>>> {
  let headers = headers_from_pairs(ctx, headers)?;
  Class::instance(ctx.clone(), Request { body: BodyState::new(body), method, url, headers })
}

pub(crate) fn init_request(ctx: &Ctx<'_>) {
  Class::<Request>::define(&ctx.globals()).expect("define Request class");
}
