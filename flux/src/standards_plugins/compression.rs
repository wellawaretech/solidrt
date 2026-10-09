//! The native codec behind `CompressionStream` and `DecompressionStream`:
//! the two classes are TransformStreams built in streams.js over a
//! `DeflateCodec`, and this is the thin layer that marshals a chunk in and
//! a piece of output out, one step on the codec threads per call
//! (`forge::compression`). Not a global: streams.rs hands the constructor
//! to streams.js (okf/done/compression-streams.md).

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use forge::compression::{Codec, Direction, Format, Step};
use rquickjs::class::Trace;
use rquickjs::function::Constructor;
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, Exception, JsLifetime, TypedArray, Value};

use crate::pending::PendingOps;
use crate::plugins::js_error::JsTypeResult;
use crate::plugins::marshal::CopyBytes;
use crate::standards_plugins::body::JsBytes;

/// The codec of one stream. `inner` is empty while a step runs on the codec
/// threads (the codec travels with the job) and after a step failed (the
/// stream is errored then, and nothing is called again).
#[derive(JsLifetime)]
#[rquickjs::class(rename = "DeflateCodec")]
pub struct DeflateCodec {
  /// The stream class the codec serves, for its messages.
  #[qjs(skip_trace)]
  api: &'static str,
  #[qjs(skip_trace)]
  inner: Rc<RefCell<Option<Codec>>>,
}

impl<'js> Trace<'js> for DeflateCodec {
  fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

#[rquickjs::methods]
impl DeflateCodec {
  /// `new DeflateCodec(direction, format)`: `direction` is "compress" or
  /// "decompress" (streams.js's word, so another is a bug); `format` is
  /// what the app gave the stream's constructor, and anything but one of
  /// the formats is the standard's TypeError.
  #[qjs(constructor)]
  pub fn new<'js>(ctx: Ctx<'js>, direction: String, format: Value<'js>) -> rquickjs::Result<Self> {
    let (direction, api) = match direction.as_str() {
      "compress" => (Direction::Compress, "CompressionStream"),
      "decompress" => (Direction::Decompress, "DecompressionStream"),
      other => return Err(Exception::throw_message(&ctx, &format!("DeflateCodec: unknown direction '{other}'"))),
    };
    let name = match format.as_string() {
      Some(s) => Some(s.to_string()?),
      None => None,
    };
    let Some(parsed) = name.as_deref().and_then(Format::parse) else {
      let given = match name {
        Some(name) => format!("\"{name}\""),
        None => "a non-string".to_string(),
      };
      return Err(Exception::throw_type(&ctx, &format!("{api}: unsupported format {given} ({})", Format::NAMES)));
    };
    Ok(DeflateCodec { api, inner: Rc::new(RefCell::new(Some(Codec::new(parsed, direction)))) })
  }

  /// Whether `next()` would produce more output for what the codec holds.
  #[qjs(get)]
  pub fn pending(&self) -> bool {
    self.inner.borrow().as_ref().is_some_and(Codec::pending)
  }

  /// Take `chunk` and produce the next piece of output.
  pub fn push<'js>(
    &self,
    ctx: Ctx<'js>,
    chunk: TypedArray<'js, u8>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsTypeResult<JsBytes>>>> {
    self.run(ctx, Step::Push(chunk.copy_bytes()))
  }

  /// The next piece of output for what the codec holds.
  pub fn next<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsTypeResult<JsBytes>>>> {
    self.run(ctx, Step::Next)
  }

  /// End the input and produce the next piece of the flush.
  pub fn finish<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsTypeResult<JsBytes>>>> {
    self.run(ctx, Step::Finish)
  }
}

impl DeflateCodec {
  /// One step on the codec threads: the engine is held in flight until the
  /// piece is back, and the codec is in its slot again before the promise
  /// settles. A failed step rejects with the standard's TypeError and
  /// leaves the slot empty.
  fn run<'js>(
    &self,
    ctx: Ctx<'js>,
    step: Step,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsTypeResult<JsBytes>>>> {
    let Some(codec) = self.inner.borrow_mut().take() else {
      return Err(Exception::throw_message(&ctx, &format!("{}: the codec is stepping or has failed", self.api)));
    };
    let api = self.api;
    let slot = self.inner.clone();
    let hold = PendingOps::of(&ctx).in_flight("compression");
    Ok(Promised(async move {
      let result = codec.step_queued(step).await;
      drop(hold);
      JsTypeResult(match result {
        Ok((codec, piece)) => {
          *slot.borrow_mut() = Some(codec);
          Ok(JsBytes(piece))
        }
        Err(e) => Err(format!("{api}: {e}")),
      })
    }))
  }
}

/// The constructor streams.js builds the two stream classes over.
pub(crate) fn constructor<'js>(ctx: &Ctx<'js>) -> Constructor<'js> {
  Class::<DeflateCodec>::create_constructor(ctx)
    .expect("create the DeflateCodec constructor")
    .expect("DeflateCodec has a constructor")
}
