use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, JsLifetime, Object, Value};

use crate::plugins::body::{body_bytes, body_json, body_text, extract_streaming_body, BodyState, JsBytes, JsonValue};
use crate::plugins::headers::{headers_from_init, headers_from_pairs, Headers};

#[derive(JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct Response<'js> {
  #[qjs(skip_trace)]
  pub(crate) body: BodyState,
  /// An async-iterable body source (e.g. an `async function*`). When `Some`, the
  /// buffered `body` is unused and the response is streamed to the client.
  pub(crate) stream: Option<Object<'js>>,
  pub(crate) status: u16,
  #[qjs(skip_trace)]
  pub(crate) status_text: String,
  pub(crate) headers: Class<'js, Headers>,
  #[qjs(skip_trace)]
  pub(crate) url: String,
}

impl<'js> Trace<'js> for Response<'js> {
  fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
    self.headers.trace(tracer);
    if let Some(stream) = &self.stream {
      stream.trace(tracer);
    }
  }
}

#[rquickjs::methods]
impl<'js> Response<'js> {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>, body: Opt<Value<'js>>, init: Opt<Object<'js>>) -> rquickjs::Result<Self> {
    let (body_bytes, stream) = match body.0 {
      Some(v) => extract_streaming_body(&ctx, &v)?,
      None => (Vec::new(), None),
    };
    let (status, status_text, headers_val) = parse_init(init.0.as_ref())?;
    let headers = headers_from_init(&ctx, headers_val.as_ref())?;
    Ok(Response { body: BodyState::new(body_bytes), stream, status, status_text, headers, url: String::new() })
  }

  #[qjs(static, rename = "json")]
  pub fn json_static(ctx: Ctx<'js>, val: Value<'js>, init: Opt<Object<'js>>) -> rquickjs::Result<Self> {
    let json = ctx.json_stringify(val)?.map(|s| s.to_string()).transpose()?.unwrap_or_else(|| "null".to_string());
    let (status, status_text, headers_val) = parse_init(init.0.as_ref())?;
    let headers = headers_from_init(&ctx, headers_val.as_ref())?;
    {
      let h = headers.borrow();
      if !h.has("content-type".to_string()) {
        h.set("Content-Type".to_string(), "application/json".to_string());
      }
    }
    Ok(Response { body: BodyState::new(json.into_bytes()), stream: None, status, status_text, headers, url: String::new() })
  }

  #[qjs(get)]
  pub fn status(&self) -> u16 {
    self.status
  }

  #[qjs(get, rename = "statusText")]
  pub fn status_text(&self) -> String {
    self.status_text.clone()
  }

  #[qjs(get)]
  pub fn ok(&self) -> bool {
    self.status >= 200 && self.status < 300
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

/// Build a Response instance directly from Rust state (used by fetch.rs).
pub(crate) fn response_from_parts<'js>(
  ctx: &Ctx<'js>,
  body: Vec<u8>,
  status: u16,
  status_text: String,
  url: String,
  headers: Vec<(String, String)>,
) -> rquickjs::Result<Class<'js, Response<'js>>> {
  let headers = headers_from_pairs(ctx, headers)?;
  Class::instance(ctx.clone(), Response { body: BodyState::new(body), stream: None, status, status_text, headers, url })
}

fn parse_init<'js>(init: Option<&Object<'js>>) -> rquickjs::Result<(u16, String, Option<Value<'js>>)> {
  let status: u16 = init.and_then(|o| o.get("status").ok()).unwrap_or(200);
  let status_text: String = init.and_then(|o| o.get("statusText").ok()).unwrap_or_default();
  let headers_val = init.and_then(|o| o.get::<_, Value>("headers").ok());
  Ok((status, status_text, headers_val))
}

pub(crate) fn init_response(ctx: &Ctx<'_>) {
  Class::<Response>::define(&ctx.globals()).expect("define Response class");
}
