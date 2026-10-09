use rquickjs::class::Trace;
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::OptArg;
use crate::standards_plugins::body::{
  body_read_all, body_stream, collect_array_buffer, collect_bytes, collect_json, collect_text, extract_streaming_body,
  BodyRead, ByteStream, JsArrayBuffer, JsBytes, JsonValue, MessageBody,
};
use crate::standards_plugins::headers::{headers_from_init, headers_from_pairs, Headers};

#[derive(JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct Response<'js> {
  /// The readable body: buffered bytes or a streamed (incoming) network body. A
  /// body held as a JS `ReadableStream` lives in `stream`, not here.
  #[qjs(skip_trace)]
  pub(crate) body: MessageBody,
  /// The body as a `ReadableStream`: given to the constructor
  /// (`new Response(stream)`, the buffered `body` then empty) or handed out
  /// by `.body`, which keeps it so every read sees the same stream. Served
  /// to the client as it is produced, or drained by a reader (`text()`),
  /// which takes it.
  pub(crate) stream: RefCell<Option<Object<'js>>>,
  pub(crate) status: u16,
  #[qjs(skip_trace)]
  pub(crate) status_text: String,
  pub(crate) headers: Class<'js, Headers>,
  #[qjs(skip_trace)]
  pub(crate) url: String,
}

type BodyFuture<'js, T> = Promised<Pin<Box<dyn Future<Output = JsResult<T>> + 'js>>>;

impl<'js> Trace<'js> for Response<'js> {
  fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
    self.headers.trace(tracer);
    if let Some(stream) = &*self.stream.borrow() {
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
    Ok(Response { body, stream: RefCell::new(stream), status, status_text, headers, url: String::new() })
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
    Ok(Response { body, stream: RefCell::new(None), status, status_text, headers, url: String::new() })
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

  /// The body as a `ReadableStream` of `Uint8Array` chunks, the same object
  /// on every read. A streamed response reads the network stream; a
  /// buffered one yields its bytes as one chunk; a stream given to the
  /// constructor is returned as-is. `for await` ready.
  #[qjs(get)]
  pub fn body(&self, ctx: Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    body_stream(&ctx, &self.body, &self.stream)
  }

  pub fn text(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<'js, String>> {
    Ok(Promised(Box::pin(collect_text(self.read_all(&ctx)?))))
  }

  pub fn bytes(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<'js, JsBytes>> {
    Ok(Promised(Box::pin(collect_bytes(self.read_all(&ctx)?))))
  }

  #[qjs(rename = "arrayBuffer")]
  pub fn array_buffer(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<'js, JsArrayBuffer>> {
    Ok(Promised(Box::pin(collect_array_buffer(self.read_all(&ctx)?))))
  }

  pub fn json(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<'js, JsonValue>> {
    Ok(Promised(Box::pin(collect_json(self.read_all(&ctx)?))))
  }
}

impl<'js> Response<'js> {
  fn read_all(&self, ctx: &Ctx<'js>) -> rquickjs::Result<BodyRead<'js>> {
    body_read_all(ctx, &self.body, &self.stream)
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
  Class::instance(ctx.clone(), Response { body, stream: RefCell::new(None), status, status_text, headers, url })
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
