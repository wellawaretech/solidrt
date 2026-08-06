//! JS bindings for sound playback: a thin marshaling layer over alloy::audio.
//! Loading (`load`, `loadPcm`, `stream`) yields a clip handle; starting a clip
//! (or the fire-and-forget `play`) yields a playback handle with live controls.
//! Raw sound/track ids stay in Rust.

use std::rc::Rc;

use alloy::audio::PcmFormat;
use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, FromJs, Function, JsLifetime, Object, TypedArray, Value};

use super::AlloyContext;
use crate::plugins::seekable::SeekableSource;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

#[derive(Clone, JsLifetime)]
struct AudioPluginState(#[qjs(skip_trace)] Rc<AlloyContext>);

/// Store the audio plugin state in userdata, before any module import, so
/// `AudioModule::evaluate` can read it. The `flux:audio` surface is registered
/// separately via `module_override`.
pub fn store_state(ctx: &Ctx<'_>, atx: AlloyContext) {
  ctx.store_userdata(AudioPluginState(Rc::new(atx))).expect("store audio state");
}

/// The `flux:audio` module. Handles are bound objects (playback: `{ stop,
/// setGain, setPan, ended }`; clip: `{ play, unload }`) so raw ids stay in Rust.
pub struct AudioModule;

impl ModuleDef for AudioModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("play")?;
    decl.declare("load")?;
    decl.declare("loadPcm")?;
    decl.declare("stream")?;
    decl.declare("stop")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("play", Function::new(ctx.clone(), play_impl)?)?;
    exports.export("load", Function::new(ctx.clone(), load_impl)?)?;
    exports.export("loadPcm", Function::new(ctx.clone(), load_pcm_impl)?)?;
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

/// Read the shared `{ loop?, gain?, pan? }` play options.
fn read_options(ctx: &Ctx<'_>, options: &Opt<Object<'_>>) -> rquickjs::Result<(bool, f32, Option<f32>)> {
  let mut looping = false;
  let mut gain = 1.0f32;
  let mut pan = None;
  if let Some(opts) = &options.0 {
    looping = opts.get::<_, Option<bool>>("loop")?.unwrap_or(false);
    gain = opts.get::<_, Option<f32>>("gain")?.unwrap_or(1.0);
    pan = opts.get::<_, Option<f32>>("pan")?;
  }
  check_gain(ctx, "play", gain)?;
  if let Some(pan) = pan {
    check_pan(ctx, "play", pan)?;
  }
  Ok((looping, gain, pan))
}

fn check_gain(ctx: &Ctx<'_>, who: &str, gain: f32) -> rquickjs::Result<()> {
  if !gain.is_finite() || gain < 0.0 {
    return Err(throw_str(ctx, &format!("{who}: gain must be a finite number >= 0")));
  }
  Ok(())
}

fn check_pan(ctx: &Ctx<'_>, who: &str, pan: f32) -> rquickjs::Result<()> {
  if !pan.is_finite() {
    return Err(throw_str(ctx, &format!("{who}: pan must be a finite number")));
  }
  Ok(())
}

/// Borrow a TypedArray's bytes. The caller only decodes during the call, so the
/// borrow need not outlive it.
fn typed_bytes<'a>(ctx: &Ctx<'_>, data: &'a TypedArray<'_, u8>, who: &str) -> rquickjs::Result<&'a [u8]> {
  let raw = data.as_raw().ok_or_else(|| throw_str(ctx, &format!("{who}: detached buffer")))?;
  Ok(unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) })
}

/// Wrap a started track id in a playback handle (`{ stop, setGain, setPan,
/// ended }`) so the raw id stays in Rust.
fn playback_handle<'js>(ctx: &Ctx<'js>, track_id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("stop", Function::new(ctx.clone(), move |ctx: Ctx<'_>| stop_impl(ctx, track_id))?)?;
  obj.set(
    "setGain",
    Function::new(ctx.clone(), move |ctx: Ctx<'_>, gain: f32| set_gain_impl(ctx, track_id, gain))?,
  )?;
  obj.set(
    "setPan",
    Function::new(ctx.clone(), move |ctx: Ctx<'_>, pan: f32| set_pan_impl(ctx, track_id, pan))?,
  )?;
  obj.set("ended", Function::new(ctx.clone(), move |ctx: Ctx<'_>| ended_impl(ctx, track_id))?)?;
  Ok(obj)
}

/// play(bytes, { loop?, gain?, pan? }) -> playback. Fire-and-forget: decodes and
/// starts in one call. Use `load` to replay a clip without re-decoding.
fn play_impl<'js>(
  ctx: Ctx<'js>,
  data: TypedArray<'js, u8>,
  options: Opt<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let (looping, gain, pan) = read_options(&ctx, &options)?;
  let bytes = typed_bytes(&ctx, &data, "play")?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_audio(bytes, looping, gain, pan).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  playback_handle(&ctx, id)
}

/// Wrap a loaded sound id in a clip handle (`{ play, unload }`), keeping the id
/// in Rust.
fn clip_handle<'js>(ctx: &Ctx<'js>, sound_id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  let play_fn =
    Function::new(ctx.clone(), object_builder(move |ctx, options| play_sound_impl(ctx, sound_id, options)))?;
  obj.set("play", play_fn)?;
  let unload_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| unload_impl(ctx, sound_id))?;
  obj.set("unload", unload_fn)?;
  Ok(obj)
}

/// load(bytes) -> clip. Decodes the clip once; each `play` starts a fresh
/// overlapping playback with no decode.
fn load_impl<'js>(ctx: Ctx<'js>, data: TypedArray<'js, u8>) -> rquickjs::Result<Object<'js>> {
  let bytes = typed_bytes(&ctx, &data, "load")?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let sound_id = state.0.load_sound(bytes).map_err(|e| throw_str(&ctx, &format!("load: {e}")))?;
  clip_handle(&ctx, sound_id)
}

/// The typed-array kinds `loadPcm` accepts; the array type is the sample
/// format. Held (not just borrowed) so the bytes stay alive until the copy.
enum PcmData<'js> {
  U8(TypedArray<'js, u8>),
  S16(TypedArray<'js, i16>),
  F32(TypedArray<'js, f32>),
}

/// loadPcm(data, sampleRate, { channels? }) -> clip. Raw samples, no decode:
/// Uint8Array = u8, Int16Array = s16, Float32Array = f32 (as laid out in
/// memory, interleaved when stereo). `channels` defaults to 1.
fn load_pcm_impl<'js>(
  ctx: Ctx<'js>,
  data: Value<'js>,
  sample_rate: f64,
  options: Opt<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let data = if let Ok(a) = TypedArray::<u8>::from_js(&ctx, data.clone()) {
    PcmData::U8(a)
  } else if let Ok(a) = TypedArray::<i16>::from_js(&ctx, data.clone()) {
    PcmData::S16(a)
  } else if let Ok(a) = TypedArray::<f32>::from_js(&ctx, data) {
    PcmData::F32(a)
  } else {
    return Err(throw_str(&ctx, "loadPcm: data must be a Uint8Array, Int16Array or Float32Array"));
  };
  if !sample_rate.is_finite() || sample_rate < 1.0 || sample_rate > i32::MAX as f64 || sample_rate.fract() != 0.0 {
    return Err(throw_str(&ctx, "loadPcm: sampleRate must be a positive integer"));
  }
  let channels = match &options.0 {
    Some(opts) => opts.get::<_, Option<i32>>("channels")?.unwrap_or(1),
    None => 1,
  };
  if channels != 1 && channels != 2 {
    return Err(throw_str(&ctx, "loadPcm: channels must be 1 or 2"));
  }
  let (bytes, format) = match &data {
    PcmData::U8(a) => (a.as_bytes(), PcmFormat::U8),
    PcmData::S16(a) => (a.as_bytes(), PcmFormat::S16),
    PcmData::F32(a) => (a.as_bytes(), PcmFormat::F32),
  };
  let bytes = bytes.ok_or_else(|| throw_str(&ctx, "loadPcm: detached buffer"))?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let sound_id = state
    .0
    .load_pcm_sound(bytes, sample_rate as i32, channels, format)
    .map_err(|e| throw_str(&ctx, &format!("loadPcm: {e}")))?;
  clip_handle(&ctx, sound_id)
}

/// stream(source) -> clip. `source` is a `file()` from `flux:fs`; its native
/// seekable source is opened for on-demand decoding (large tracks stay off the
/// heap) and fed to SDL_mixer as a custom byte source. Taking the file object
/// rather than a path means streaming rides the file's attached backend: a
/// packed asset streams via range reads out of the exe instead of a plain disk
/// file. Play it as a single voice; do not overlap a stream with itself.
fn stream_impl<'js>(ctx: Ctx<'js>, source: Object<'js>) -> rquickjs::Result<Object<'js>> {
  let reader = SeekableSource::open_from(&source).map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let sound_id = state.0.stream_sound_io(reader).map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  clip_handle(&ctx, sound_id)
}

fn play_sound_impl<'js>(ctx: Ctx<'js>, sound_id: u64, options: Opt<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let (looping, gain, pan) = read_options(&ctx, &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_sound(sound_id, looping, gain, pan).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  playback_handle(&ctx, id)
}

fn set_gain_impl(ctx: Ctx<'_>, id: u64, gain: f32) -> rquickjs::Result<()> {
  check_gain(&ctx, "setGain", gain)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.set_audio_gain(id, gain).map_err(|e| throw_str(&ctx, &format!("setGain: {e}")))
}

fn set_pan_impl(ctx: Ctx<'_>, id: u64, pan: f32) -> rquickjs::Result<()> {
  check_pan(&ctx, "setPan", pan)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.set_audio_pan(id, pan).map_err(|e| throw_str(&ctx, &format!("setPan: {e}")))
}

fn ended_impl(ctx: Ctx<'_>, id: u64) -> bool {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.audio_ended(id)
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
