// The WHATWG Streams subset: `ReadableStream`, `WritableStream` and
// `TransformStream`, plus `TextDecoderStream` and `TextEncoderStream`, and
// the Compression Streams API's `CompressionStream` and
// `DecompressionStream`. The classes are plain JS (streams.js: promise and
// queue plumbing, short there and miserable in rquickjs), evaluated once per
// context here and installed as globals; the compression streams are built
// over the native `DeflateCodec` (compression.rs) the source is handed.
// `readable_from` is what the body and subprocess plugins wrap a native
// byte source with (okf/done/web-streams.md); `writable_to` is the dual for
// a native byte sink (a socket's write half, a child's stdin), what the
// duplexes of flux:net, flux:p2p and flux:subprocess hand out as `writable`
// (okf/done/flux-duplex-streams.md).

use std::future::Future;
use std::pin::Pin;

use rquickjs::context::EvalOptions;
use rquickjs::function::{Constructor, MutFn, This};
use rquickjs::promise::Promised;
use rquickjs::{Coerced, Ctx, FromJs, Function, Object, Value};

use crate::plugins::marshal::{with_in_flight, OptArg};
use crate::standards_plugins::body::extract_body_value;

const SOURCE: &str = include_str!("streams.js");

/// The file name stack frames inside the implementation cite.
const FILE_NAME: &str = "flux:streams";

const GLOBALS: &[&str] = &[
  "ReadableStream",
  "WritableStream",
  "TransformStream",
  "TextDecoderStream",
  "TextEncoderStream",
  "CompressionStream",
  "DecompressionStream",
];

pub(crate) fn init_streams(ctx: &Ctx<'_>) {
  let mut options = EvalOptions::default();
  options.filename = Some(FILE_NAME.to_string());
  let build: Function = ctx.eval_with_options(SOURCE, options).expect("evaluate streams.js");
  let codec = crate::standards_plugins::compression::constructor(ctx);
  let api: Object = build.call((codec,)).expect("build the streams classes");
  for name in GLOBALS {
    let class: Value = api.get(*name).expect("streams class");
    ctx.globals().set(*name, class).expect("install streams global");
  }
}

/// `iterable` as a `ReadableStream`: itself when it already is one, else
/// `ReadableStream.from(iterable)`, which pulls one item per read and
/// returns the iterator when the stream is cancelled.
pub(crate) fn readable_from<'js>(ctx: &Ctx<'js>, iterable: Object<'js>) -> rquickjs::Result<Object<'js>> {
  let class: Object<'js> = ctx.globals().get("ReadableStream")?;
  if iterable.is_instance_of(&class) {
    return Ok(iterable);
  }
  let from: Function<'js> = class.get("from")?;
  from.call((This(class), iterable))
}

/// One step of a native sink: a write, a close or an abort, done when it
/// resolves. Not `Send`: it runs on the JS executor behind the promise the
/// sink method returns.
pub(crate) type SinkFuture = Pin<Box<dyn Future<Output = Result<(), String>>>>;

/// A `WritableStream` over a native byte sink. Its `write(chunk)` decodes a
/// string or `Uint8Array` chunk (anything else errors the stream, naming
/// `api`) and awaits `write`, so the stream's queue paces a fast producer by
/// the sink's own backpressure; `close()` awaits `close` after the writes so
/// far; `abort(reason)` awaits `abort` with the reason's text. Each step
/// holds the engine as work in flight of `kind`. The closures capture Rust
/// state only (the closure-capture trap in flux/CLAUDE.md).
pub(crate) fn writable_to<'js>(
  ctx: &Ctx<'js>,
  api: &'static str,
  kind: &'static str,
  write: impl Fn(Vec<u8>) -> SinkFuture + 'static,
  close: impl Fn() -> SinkFuture + 'static,
  abort: impl Fn(String) -> SinkFuture + 'static,
) -> rquickjs::Result<Object<'js>> {
  let sink = Object::new(ctx.clone())?;
  sink.set(
    "write",
    Function::new(
      ctx.clone(),
      MutFn::from(move |ctx: Ctx<'_>, chunk: Value<'_>| -> rquickjs::Result<Promised<_>> {
        let bytes = extract_body_value(&chunk, api)?;
        Ok(with_in_flight(&ctx, kind, write(bytes)))
      }),
    )?,
  )?;
  sink.set(
    "close",
    Function::new(
      ctx.clone(),
      MutFn::from(move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> { Ok(with_in_flight(&ctx, kind, close())) }),
    )?,
  )?;
  sink.set(
    "abort",
    Function::new(
      ctx.clone(),
      MutFn::from(move |ctx: Ctx<'_>, reason: OptArg<Value<'_>>| -> rquickjs::Result<Promised<_>> {
        let reason = reason_text(reason.0);
        Ok(with_in_flight(&ctx, kind, abort(reason)))
      }),
    )?,
  )?;
  let class: Constructor<'js> = ctx.globals().get("WritableStream")?;
  class.construct((sink,))
}

/// What an abort with no reason reports.
const NO_REASON: &str = "aborted";

/// An abort reason as text: an `Error`'s message, else the value coerced to
/// a string, else `NO_REASON`.
fn reason_text<'js>(reason: Option<Value<'js>>) -> String {
  let Some(reason) = reason else {
    return NO_REASON.to_string();
  };
  let ctx = reason.ctx().clone();
  if let Some(obj) = reason.as_object() {
    if let Ok(Some(message)) = obj.get::<_, Option<Coerced<String>>>("message") {
      return message.0;
    }
  }
  Coerced::<String>::from_js(&ctx, reason).map(|s| s.0).unwrap_or_else(|_| NO_REASON.to_string())
}
