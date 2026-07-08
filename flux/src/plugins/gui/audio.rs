//! JS bindings for sound playback: a thin marshaling layer over alloy::audio.
//! `play` decodes an encoded clip (Ogg/Vorbis or WAV) and returns a bound handle
//! (`{ stop }`) so the raw track id stays in Rust.

use std::rc::Rc;

use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, JsLifetime, Object, TypedArray};

use super::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

#[derive(Clone, JsLifetime)]
struct AudioPluginState(#[qjs(skip_trace)] Rc<AlloyContext>);

/// Store the audio plugin state in userdata, before any module import, so
/// `AudioModule::evaluate` can read it. The `flux:audio` surface is registered
/// separately via `module_override`.
pub fn store_state(ctx: &Ctx<'_>, atx: AlloyContext) {
  ctx.store_userdata(AudioPluginState(Rc::new(atx))).expect("store audio state");
}

/// The `flux:audio` module. `play` returns a bound handle object (`{ stop }`)
/// so the raw track id stays in Rust.
pub struct AudioModule;

impl ModuleDef for AudioModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("play")?;
    decl.declare("stopAll")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("play", Function::new(ctx.clone(), play_impl)?)?;
    exports.export("stopAll", Function::new(ctx.clone(), stop_all_impl)?)?;
    Ok(())
  }
}

/// play(bytes, { loop?, gain? }) -> { stop() }
fn play_impl<'js>(
  ctx: Ctx<'js>,
  data: TypedArray<'js, u8>,
  options: Opt<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let mut looping = false;
  let mut gain = 1.0f32;
  if let Some(opts) = options.0 {
    looping = opts.get::<_, Option<bool>>("loop")?.unwrap_or(false);
    gain = opts.get::<_, Option<f32>>("gain")?.unwrap_or(1.0);
  }

  let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "play: detached buffer"))?;
  // `play_audio` fully decodes the clip during the call (predecode), so the
  // borrowed bytes need not outlive it.
  let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };

  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_audio(bytes, looping, gain).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;

  let obj = Object::new(ctx.clone())?;
  // stop() is bound to this track id so the raw handle stays in Rust.
  let stop_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| stop_impl(ctx, id))?;
  obj.set("stop", stop_fn)?;
  Ok(obj)
}

fn stop_impl(ctx: Ctx<'_>, id: u64) {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.stop_audio(id);
}

fn stop_all_impl(ctx: Ctx<'_>) {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.stop_all_audio();
}
