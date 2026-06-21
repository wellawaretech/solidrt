//! JS bindings for microphone capture: a thin marshaling layer over
//! alloy::microphone. Open is synchronous (SDL exposes no permission state
//! for audio recording); the JS wrapper in @solidrt/core makes it async to
//! leave room for platforms that need a runtime permission flow.

use std::rc::Rc;

use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Function, JsLifetime, Object, TypedArray};

use super::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

#[derive(Clone, JsLifetime)]
struct MicrophonePluginState(#[qjs(skip_trace)] Rc<AlloyContext>);

/// Store the microphone plugin state in userdata, before any module import, so
/// `MicrophoneModule::evaluate` can read it. The `flux:microphone` surface is
/// registered separately via `module_override`.
pub fn store_state(ctx: &Ctx<'_>, atx: AlloyContext) {
  ctx.store_userdata(MicrophonePluginState(Rc::new(atx))).expect("store microphone state");
}

/// The `flux:microphone` module. `open` returns a bound session object
/// (`{ sampleRate, read, close }`) so the raw handle stays in Rust.
pub struct MicrophoneModule;

impl ModuleDef for MicrophoneModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("listMicrophones")?;
    decl.declare("open")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("listMicrophones", Function::new(ctx.clone(), list_impl)?)?;
    exports.export("open", Function::new(ctx.clone(), open_impl)?)?;
    Ok(())
  }
}

fn list_impl(ctx: Ctx<'_>) -> rquickjs::Result<Array<'_>> {
  let arr = Array::new(ctx.clone())?;
  for (i, mic) in alloy::microphone::list_microphones().iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("id", mic.id)?;
    obj.set("name", mic.name.as_str())?;
    arr.set(i, obj)?;
  }
  Ok(arr)
}

fn open_impl<'js>(ctx: Ctx<'js>, options: Opt<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let mut device: Option<u32> = None;
  let mut sample_rate: Option<u32> = None;
  if let Some(opts) = options.0 {
    device = opts.get("microphone")?;
    sample_rate = opts.get("sampleRate")?;
  }
  let sample_rate = sample_rate.unwrap_or(16000);

  let state = ctx.userdata::<MicrophonePluginState>().expect("microphone state");
  let session =
    state.0.open_microphone(device, sample_rate).map_err(|e| throw_str(&ctx, &format!("openMicrophone: {e}")))?;

  let obj = Object::new(ctx.clone())?;
  obj.set("sampleRate", sample_rate)?;
  // read()/close() are bound to this session so the raw handle stays in Rust;
  // they reuse the same helpers the global API did. read returns an invariant
  // TypedArray<'js>, so its closure needs the HRTB coercion (see flux/CLAUDE.md);
  // close returns () and does not.
  let read_fn = Function::new(ctx.clone(), read_builder(move |ctx| read_impl(ctx, session)))?;
  obj.set("read", read_fn)?;
  let close_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_impl(ctx, session))?;
  obj.set("close", close_fn)?;
  Ok(obj)
}

// Coerces a capturing closure to the `for<'js>` HRTB rquickjs needs to return an
// invariant `TypedArray<'js>`; a capturing closure will not infer it on its own.
fn read_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>) -> rquickjs::Result<TypedArray<'js, f32>>,
{
  f
}

/// Drain the mono f32 samples captured since the last read.
fn read_impl(ctx: Ctx<'_>, session: u64) -> rquickjs::Result<TypedArray<'_, f32>> {
  let state = ctx.userdata::<MicrophonePluginState>().expect("microphone state");
  let samples = state.0.read_microphone(session).map_err(|e| throw_str(&ctx, &format!("read: {e}")))?;
  TypedArray::new(ctx.clone(), samples)
}

fn close_impl(ctx: Ctx<'_>, session: u64) {
  let state = ctx.userdata::<MicrophonePluginState>().expect("microphone state");
  state.0.close_microphone(session);
}
