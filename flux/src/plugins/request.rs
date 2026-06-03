use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, JsLifetime, Object, Value};
use std::future::Future;
use std::pin::Pin;

use crate::pending::PendingOps;
use crate::plugins::body::{
  collect_bytes, collect_json, collect_text, extract_body_value, BodySource, ByteStream, JsBytes, JsonValue,
  MessageBody,
};
use crate::plugins::headers::{headers_from_init, headers_from_pairs, Headers};

type BodyFuture<T> = Promised<Pin<Box<dyn Future<Output = rquickjs::Result<T>>>>>;

#[derive(JsLifetime)]
#[rquickjs::class(rename = "Request")]
pub struct Request<'js> {
  /// The readable body: buffered (a JS-constructed Request) or a streamed network
  /// body (an incoming server request, read incrementally). Shared with Response.
  #[qjs(skip_trace)]
  pub(crate) body: MessageBody,
  #[qjs(skip_trace)]
  pub(crate) method: String,
  #[qjs(skip_trace)]
  pub(crate) url: String,
  pub(crate) headers: Class<'js, Headers>,
  // Matched path parameters, populated by the router (empty object otherwise).
  pub(crate) params: Object<'js>,
}

impl<'js> Trace<'js> for Request<'js> {
  fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
    self.headers.trace(tracer);
    self.params.trace(tracer);
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
    let params = Object::new(ctx.clone())?;
    Ok(Request { body: MessageBody::buffered(body_bytes), method, url, headers, params })
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

  #[qjs(get)]
  pub fn params(&self) -> Object<'js> {
    self.params.clone()
  }

  /// The body as an async-iterable of `Uint8Array` chunks. An incoming request
  /// iterates its network stream (read incrementally, constant-memory for large
  /// uploads); a buffered one yields its bytes as one chunk. `for await` ready.
  #[qjs(get)]
  pub fn body(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
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

  pub fn json(&self, ctx: Ctx<'js>) -> rquickjs::Result<BodyFuture<JsonValue>> {
    let (source, pending) = self.reader(&ctx)?;
    Ok(Promised(Box::pin(collect_json(source, pending))))
  }
}

impl<'js> Request<'js> {
  /// Consume the body once for a reader (`text`/`bytes`/`json`), returning the
  /// drainable source and a `PendingOps` handle.
  fn reader(&self, ctx: &Ctx<'js>) -> rquickjs::Result<(BodySource, PendingOps)> {
    let source = self.body.take_source(ctx)?;
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    Ok((source, pending))
  }
}

/// Build a Request directly from Rust state (used by serve.rs). `body` is the
/// incoming request body stream, read incrementally by the handler rather than
/// buffered up front. `params` are matched route path parameters; pass an empty
/// Vec when there is no route match.
pub(crate) fn request_from_parts<'js>(
  ctx: &Ctx<'js>,
  method: String,
  url: String,
  body: ByteStream,
  headers: Vec<(String, String)>,
  params: Vec<(String, String)>,
) -> rquickjs::Result<Class<'js, Request<'js>>> {
  let headers = headers_from_pairs(ctx, headers)?;
  let params_obj = Object::new(ctx.clone())?;
  for (k, v) in params {
    params_obj.set(k, v)?;
  }
  Class::instance(ctx.clone(), Request { body: MessageBody::incoming(body), method, url, headers, params: params_obj })
}

pub(crate) fn init_request(ctx: &Ctx<'_>) {
  Class::<Request>::define(&ctx.globals()).expect("define Request class");
}
