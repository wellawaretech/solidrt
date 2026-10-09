// The WHATWG Streams subset: `ReadableStream`, `WritableStream` and
// `TransformStream`, plus `TextDecoderStream` and `TextEncoderStream`. The
// classes are plain JS (streams.js: promise and queue plumbing, short there
// and miserable in rquickjs), evaluated once per context here and installed
// as globals. `readable_from` is what the body and subprocess plugins wrap a
// native byte source with (okf/done/web-streams.md).

use rquickjs::context::EvalOptions;
use rquickjs::function::This;
use rquickjs::{Ctx, Function, Object, Value};

const SOURCE: &str = include_str!("streams.js");

/// The file name stack frames inside the implementation cite.
const FILE_NAME: &str = "flux:streams";

const GLOBALS: &[&str] =
  &["ReadableStream", "WritableStream", "TransformStream", "TextDecoderStream", "TextEncoderStream"];

pub(crate) fn init_streams(ctx: &Ctx<'_>) {
  let mut options = EvalOptions::default();
  options.filename = Some(FILE_NAME.to_string());
  let build: Function = ctx.eval_with_options(SOURCE, options).expect("evaluate streams.js");
  let api: Object = build.call(()).expect("build the streams classes");
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
