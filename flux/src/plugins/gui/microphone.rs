//! JS bindings for microphone capture: a thin marshaling layer over
//! alloy::microphone. Open is synchronous (SDL exposes no permission state
//! for audio recording); the JS wrapper in @solidrt/core makes it async to
//! leave room for platforms that need a runtime permission flow.

use std::rc::Rc;

use rquickjs::function::Opt;
use rquickjs::{Array, Ctx, Function, JsLifetime, Object, TypedArray};

use super::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

#[derive(Clone, JsLifetime)]
struct MicrophonePluginState(#[qjs(skip_trace)] Rc<AlloyContext>);

pub fn init(ctx: Ctx<'_>, atx: AlloyContext) {
  ctx.store_userdata(MicrophonePluginState(Rc::new(atx))).expect("store microphone state");

  let list = Function::new(ctx.clone(), list_impl).expect("create microphone.listMicrophones");
  let open = Function::new(ctx.clone(), open_impl).expect("create microphone.open");
  let read = Function::new(ctx.clone(), read_impl).expect("create microphone.read");
  let close = Function::new(ctx.clone(), close_impl).expect("create microphone.close");

  let microphone = Object::new(ctx.clone()).expect("create microphone object");
  microphone.set("listMicrophones", list).expect("set microphone.listMicrophones");
  microphone.set("open", open).expect("set microphone.open");
  microphone.set("read", read).expect("set microphone.read");
  microphone.set("close", close).expect("set microphone.close");
  ctx.globals().set("microphone", microphone).expect("set microphone global");
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
  obj.set("handle", session)?;
  obj.set("sampleRate", sample_rate)?;
  Ok(obj)
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
