//! JS bindings for sound playback: a thin marshaling layer over alloy::audio.
//! `play` decodes an encoded clip (Ogg/Vorbis or WAV) and returns a bound handle
//! (`{ stop }`) so the raw track id stays in Rust.

use std::rc::Rc;

use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Function, JsLifetime, Object, TypedArray};

use super::AlloyContext;
use crate::plugins::seekable::SeekableSource;

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
    decl.declare("load")?;
    decl.declare("stream")?;
    decl.declare("stop")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("play", Function::new(ctx.clone(), play_impl)?)?;
    exports.export("load", Function::new(ctx.clone(), load_impl)?)?;
    exports.export("stream", Function::new(ctx.clone(), stream_impl)?)?;
    exports.export("stop", Function::new(ctx.clone(), stop_all_impl)?)?;
    Ok(())
  }
}

/// Force the `for<'js>` HRTB on a capturing closure that returns a `'js`-bound
/// `Object` (see flux/CLAUDE.md "Ctx and the 'js lifetime").
fn object_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, Opt<Object<'js>>) -> rquickjs::Result<Object<'js>>,
{
  f
}

/// Read the shared `{ loop?, gain? }` play options.
fn read_options(options: &Opt<Object<'_>>) -> rquickjs::Result<(bool, f32)> {
  let mut looping = false;
  let mut gain = 1.0f32;
  if let Some(opts) = &options.0 {
    looping = opts.get::<_, Option<bool>>("loop")?.unwrap_or(false);
    gain = opts.get::<_, Option<f32>>("gain")?.unwrap_or(1.0);
  }
  Ok((looping, gain))
}

/// Borrow a TypedArray's bytes. The caller only decodes during the call, so the
/// borrow need not outlive it.
fn typed_bytes<'a>(ctx: &Ctx<'_>, data: &'a TypedArray<'_, u8>, who: &str) -> rquickjs::Result<&'a [u8]> {
  let raw = data.as_raw().ok_or_else(|| throw_str(ctx, &format!("{who}: detached buffer")))?;
  Ok(unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) })
}

/// Wrap a started track id in a `{ stop() }` handle so the raw id stays in Rust.
fn voice_handle<'js>(ctx: &Ctx<'js>, track_id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  let stop_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| stop_impl(ctx, track_id))?;
  obj.set("stop", stop_fn)?;
  Ok(obj)
}

/// play(bytes, { loop?, gain? }) -> { stop() }. Fire-and-forget: decodes and
/// starts in one call. Use `load` to replay a clip without re-decoding.
fn play_impl<'js>(
  ctx: Ctx<'js>,
  data: TypedArray<'js, u8>,
  options: Opt<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let (looping, gain) = read_options(&options)?;
  let bytes = typed_bytes(&ctx, &data, "play")?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_audio(bytes, looping, gain).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  voice_handle(&ctx, id)
}

/// Wrap a loaded sound id in a `{ play, unload }` handle, keeping the id in Rust.
fn sound_handle<'js>(ctx: &Ctx<'js>, sound_id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  let play_fn =
    Function::new(ctx.clone(), object_builder(move |ctx, options| play_sound_impl(ctx, sound_id, options)))?;
  obj.set("play", play_fn)?;
  let unload_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| unload_impl(ctx, sound_id))?;
  obj.set("unload", unload_fn)?;
  Ok(obj)
}

/// load(bytes) -> { play({ loop?, gain? }) -> { stop() }, unload() }. Decodes
/// the clip once; each `play` starts a fresh overlapping voice with no decode.
fn load_impl<'js>(ctx: Ctx<'js>, data: TypedArray<'js, u8>) -> rquickjs::Result<Object<'js>> {
  let bytes = typed_bytes(&ctx, &data, "load")?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let sound_id = state.0.load_sound(bytes).map_err(|e| throw_str(&ctx, &format!("load: {e}")))?;
  sound_handle(&ctx, sound_id)
}

/// stream(source) -> { play({ loop?, gain? }) -> { stop() }, unload() }. `source`
/// is a `file()` from `flux:fs`; its native seekable source is opened for
/// on-demand decoding (large tracks stay off the heap) and fed to SDL_mixer as a
/// custom byte source. Taking the file object rather than a path means streaming
/// rides the `file()` proxy override: a proxied file streams from the dev server.
/// Play it as a single voice; do not overlap a stream with itself.
fn stream_impl<'js>(ctx: Ctx<'js>, source: Object<'js>) -> rquickjs::Result<Object<'js>> {
  let reader = SeekableSource::open_from(&source).map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let sound_id = state.0.stream_sound_io(reader).map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  sound_handle(&ctx, sound_id)
}

fn play_sound_impl<'js>(ctx: Ctx<'js>, sound_id: u64, options: Opt<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let (looping, gain) = read_options(&options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_sound(sound_id, looping, gain).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  voice_handle(&ctx, id)
}

fn unload_impl(ctx: Ctx<'_>, sound_id: u64) {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.unload_sound(sound_id);
}

fn stop_impl(ctx: Ctx<'_>, id: u64) {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.stop_audio(id);
}

fn stop_all_impl(ctx: Ctx<'_>) {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.stop_all_audio();
}
