use rquickjs::class::Trace;
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, JsLifetime, Object, Value};
use std::future::Future;
use std::pin::Pin;

use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::OptArg;
use crate::standards_plugins::body::{
  collect_array_buffer, collect_bytes, collect_json, collect_text, extract_streaming_body, throw_msg, BodySource,
  ByteStream, JsArrayBuffer, JsBytes, JsonValue, MessageBody,
};
use crate::standards_plugins::headers::{headers_from_init, headers_from_pairs, Headers};

#[derive(JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct Response<'js> {
  /// The readable body: buffered bytes or a streamed (incoming) network body. The
  /// OUTGOING `async function*` case lives in `stream`, not here.
  #[qjs(skip_trace)]
  pub(crate) body: MessageBody,
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

type BodyFuture<T> = Promised<Pin<Box<dyn Future<Output = JsResult<T>>>>>;

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
  pub fn new(ctx: Ctx<'js>, body: OptArg<Value<'js>>, init: OptArg<Object<'js>>) -> rquickjs::Result<Self> {
    let (body_bytes, stream) = match body.0 {
      Some(v) => extract_streaming_body(&v)?,
      None => (Vec::new(), None),
    };
    let (status, status_text, headers_val) = parse_init(init.0.as_ref())?;
    let headers = headers_from_init(&ctx, headers_val.as_ref())?;
    let body = MessageBody::buffered(body_bytes);
    Ok(Response { body, stream, status, status_text, headers, url: String::new() })
  }

  #[qjs(static, rename = "json")]
  pub fn json_static(ctx: Ctx<'js>, val: Value<'js>, init: OptArg<Object<'js>>) -> rquickjs::Result<Self> {
    let json = ctx.json_stringify(val)?.map(|s| s.to_string()).transpose()?.unwrap_or_else(|| "null".to_string());
    let (status, status_text, headers_val) = parse_init(init.0.as_ref())?;
    let headers = headers_from_init(&ctx, headers_val.as_ref())?;
    {
      let h = headers.borrow();
      if !h.has("content-type".to_string()) {
        h.set("Content-Type".to_string(), "application/json".to_string());
      }
    }
    let body = MessageBody::buffered(json.into_bytes());
    Ok(Response { body, stream: None, status, status_text, headers, url: String::new() })
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

  /// The body as an async-iterable of `Uint8Array` chunks. A streamed response
  /// iterates the network stream; a buffered one yields its bytes as one chunk;
  /// an outgoing `async function*` body is returned as-is. `for await` ready.
  #[qjs(get)]
  pub fn body(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    if let Some(stream) = &self.stream {
      return Ok(stream.clone().into_value());
    }
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    self.body.as_async_iterable(&ctx, pending)
  }

  pub fn text(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<String>> {
    let (source, pending) = self.reader(&ctx)?;
    Ok(Promised(Box::pin(collect_text(source, pending))))
  }

  pub fn bytes(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<JsBytes>> {
    let (source, pending) = self.reader(&ctx)?;
    Ok(Promised(Box::pin(collect_bytes(source, pending))))
  }

  #[qjs(rename = "arrayBuffer")]
  pub fn array_buffer(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<JsArrayBuffer>> {
    let (source, pending) = self.reader(&ctx)?;
    Ok(Promised(Box::pin(collect_array_buffer(source, pending))))
  }

  pub fn json(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<JsonValue>> {
    let (source, pending) = self.reader(&ctx)?;
    Ok(Promised(Box::pin(collect_json(source, pending))))
  }
}

impl<'js> Response<'js> {
  /// Consume the body once for a reader (`text`/`bytes`/`json`), returning the
  /// drainable source and a `PendingOps` handle. An outgoing stream body cannot
  /// be read this way.
  fn reader(&self, ctx: &Ctx<'js>) -> rquickjs::Result<(BodySource, PendingOps)> {
    if self.stream.is_some() {
      return Err(throw_msg(ctx, "Response body is a stream; iterate response.body instead"));
    }
    let source = self.body.take_source(ctx)?;
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    Ok((source, pending))
  }
}

/// Build a Response instance directly from Rust state (used by fetch.rs). The
/// `body` is a live network stream, drained on demand (streamed by default).
pub(crate) fn response_from_parts<'js>(
  ctx: &Ctx<'js>,
  body: ByteStream,
  status: u16,
  status_text: String,
  url: String,
  headers: Vec<(String, String)>,
) -> rquickjs::Result<Class<'js, Response<'js>>> {
  let headers = headers_from_pairs(ctx, headers)?;
  let body = MessageBody::incoming(body);
  Class::instance(ctx.clone(), Response { body, stream: None, status, status_text, headers, url })
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
