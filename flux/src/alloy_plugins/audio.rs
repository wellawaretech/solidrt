//! JS bindings for sound playback: a thin marshaling layer over alloy::audio.
//! Loading (`load`, `loadPcm`, `stream`) yields a clip handle; starting a clip
//! (or the fire-and-forget `play`) yields a playback handle with live controls.
//! Raw sound/track ids stay in Rust.

use std::rc::Rc;

use alloy::audio::PcmFormat;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, FromJs, Function, JsLifetime, Object, TypedArray, Value};

use super::AlloyContext;
use crate::plugins::marshal::OptArg;
use crate::plugins::seekable::SeekableSource;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

#[derive(Clone, JsLifetime)]
struct AudioPluginState(#[qjs(skip_trace)] Rc<AlloyContext>);

/// Store the audio plugin state in userdata, before any module import, so
/// `AudioModule::evaluate` can read it. The `flux:audio` surface is registered
/// separately via `module_override`.
pub(crate) fn store_state(ctx: &Ctx<'_>, atx: AlloyContext) {
  ctx.store_userdata(AudioPluginState(Rc::new(atx))).expect("store audio state");
}

/// The `flux:audio` module. Handles are bound objects (playback: `{ stop,
/// setGain, setPan, setRate, ended }`; clip: `{ play, unload }`) so raw ids
/// stay in Rust.
pub struct AudioModule;

impl ModuleDef for AudioModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("play")?;
    decl.declare("load")?;
    decl.declare("loadPcm")?;
    decl.declare("stream")?;
    decl.declare("stop")?;
    decl.declare("setMasterGain")?;
    decl.declare("setBusGain")?;
    decl.declare("outputSampleRate")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("play", Function::new(ctx.clone(), play_impl)?)?;
    exports.export("load", Function::new(ctx.clone(), load_impl)?)?;
    exports.export("loadPcm", Function::new(ctx.clone(), load_pcm_impl)?)?;
    exports.export("stream", Function::new(ctx.clone(), stream_impl)?)?;
    exports.export("stop", Function::new(ctx.clone(), stop_all_impl)?)?;
    exports.export("setMasterGain", Function::new(ctx.clone(), set_master_gain_impl)?)?;
    exports.export("setBusGain", Function::new(ctx.clone(), set_bus_gain_impl)?)?;
    exports.export("outputSampleRate", Function::new(ctx.clone(), output_sample_rate_impl)?)?;
    Ok(())
  }
}

/// Force the `for<'js>` HRTB on a capturing closure that returns a `'js`-bound
/// `Object` (see flux/CLAUDE.md "Ctx and the 'js lifetime").
fn object_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, OptArg<Object<'js>>) -> rquickjs::Result<Object<'js>>,
{
  f
}

/// Same HRTB coercion for the capturing setter closures, whose `Ctx` and
/// options bag must share one `'js` (elision infers them independently).
fn setter_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, f32, OptArg<Object<'js>>) -> rquickjs::Result<()>,
{
  f
}

/// And for the capturing stop closure (no value argument).
fn stop_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, OptArg<Object<'js>>) -> rquickjs::Result<()>,
{
  f
}

/// Read the shared `{ loop?, gain?, pan?, rate?, fadeInMs?, bus? }` play options.
fn read_options(ctx: &Ctx<'_>, options: &OptArg<Object<'_>>) -> rquickjs::Result<alloy::audio::PlayOptions> {
  let mut parsed = alloy::audio::PlayOptions::default();
  if let Some(opts) = &options.0 {
    parsed.looping = opts.get::<_, Option<bool>>("loop")?.unwrap_or(false);
    parsed.gain = opts.get::<_, Option<f32>>("gain")?.unwrap_or(1.0);
    parsed.pan = opts.get::<_, Option<f32>>("pan")?;
    parsed.rate = opts.get::<_, Option<f32>>("rate")?;
    parsed.fade_in_ms = opts.get::<_, Option<f64>>("fadeInMs")?.unwrap_or(0.0);
    parsed.bus = opts.get::<_, Option<String>>("bus")?;
  }
  check_gain(ctx, "play", parsed.gain)?;
  if let Some(pan) = parsed.pan {
    check_pan(ctx, "play", pan)?;
  }
  if let Some(rate) = parsed.rate {
    check_rate(ctx, "play", rate)?;
  }
  check_ms(ctx, "play", "fadeInMs", parsed.fade_in_ms)?;
  if let Some(bus) = &parsed.bus {
    check_bus(ctx, "play", bus)?;
  }
  Ok(parsed)
}

fn check_bus(ctx: &Ctx<'_>, who: &str, bus: &str) -> rquickjs::Result<()> {
  if bus.is_empty() || bus.contains('\0') {
    return Err(throw_str(ctx, &format!("{who}: bus must be a non-empty string")));
  }
  Ok(())
}

/// Read the `{ rampMs? }` options bag the live setters share.
fn read_ramp_ms(ctx: &Ctx<'_>, who: &str, options: &OptArg<Object<'_>>) -> rquickjs::Result<f64> {
  let Some(opts) = &options.0 else { return Ok(0.0) };
  let ramp = opts.get::<_, Option<f64>>("rampMs")?.unwrap_or(0.0);
  check_ms(ctx, who, "rampMs", ramp)?;
  Ok(ramp)
}

/// Read the `{ fadeOutMs? }` options bag the stop calls share.
fn read_fade_out_ms(ctx: &Ctx<'_>, who: &str, options: &OptArg<Object<'_>>) -> rquickjs::Result<f64> {
  let Some(opts) = &options.0 else { return Ok(0.0) };
  let fade = opts.get::<_, Option<f64>>("fadeOutMs")?.unwrap_or(0.0);
  check_ms(ctx, who, "fadeOutMs", fade)?;
  Ok(fade)
}

fn check_ms(ctx: &Ctx<'_>, who: &str, what: &str, ms: f64) -> rquickjs::Result<()> {
  if !ms.is_finite() || ms < 0.0 {
    return Err(throw_str(ctx, &format!("{who}: {what} must be a finite number >= 0")));
  }
  Ok(())
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

fn check_rate(ctx: &Ctx<'_>, who: &str, rate: f32) -> rquickjs::Result<()> {
  if !rate.is_finite() || rate <= 0.0 {
    return Err(throw_str(ctx, &format!("{who}: rate must be a finite number > 0")));
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
/// setRate, ended }`) so the raw id stays in Rust.
fn playback_handle<'js>(ctx: &Ctx<'js>, track_id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set(
    "stop",
    Function::new(ctx.clone(), stop_builder(move |ctx, options| stop_impl(ctx, track_id, options)))?,
  )?;
  obj.set(
    "setGain",
    Function::new(ctx.clone(), setter_builder(move |ctx, gain, options| set_gain_impl(ctx, track_id, gain, options)))?,
  )?;
  obj.set(
    "setPan",
    Function::new(ctx.clone(), setter_builder(move |ctx, pan, options| set_pan_impl(ctx, track_id, pan, options)))?,
  )?;
  obj.set(
    "setRate",
    Function::new(ctx.clone(), setter_builder(move |ctx, rate, options| set_rate_impl(ctx, track_id, rate, options)))?,
  )?;
  obj.set("ended", Function::new(ctx.clone(), move |ctx: Ctx<'_>| ended_impl(ctx, track_id))?)?;
  Ok(obj)
}

/// play(bytes, { loop?, gain?, pan?, rate?, fadeInMs? }) -> playback.
/// Fire-and-forget: decodes and starts in one call. Use `load` to replay a
/// clip without re-decoding.
fn play_impl<'js>(
  ctx: Ctx<'js>,
  data: TypedArray<'js, u8>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let play_options = read_options(&ctx, &options)?;
  let bytes = typed_bytes(&ctx, &data, "play")?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_audio(bytes, &play_options).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
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
  options: OptArg<Object<'js>>,
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

fn play_sound_impl<'js>(ctx: Ctx<'js>, sound_id: u64, options: OptArg<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let play_options = read_options(&ctx, &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  let id = state.0.play_sound(sound_id, &play_options).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  playback_handle(&ctx, id)
}

fn set_gain_impl<'js>(ctx: Ctx<'js>, id: u64, gain: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_gain(&ctx, "setGain", gain)?;
  let ramp_ms = read_ramp_ms(&ctx, "setGain", &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.set_audio_gain(id, gain, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setGain: {e}")))
}

fn set_pan_impl<'js>(ctx: Ctx<'js>, id: u64, pan: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_pan(&ctx, "setPan", pan)?;
  let ramp_ms = read_ramp_ms(&ctx, "setPan", &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.set_audio_pan(id, pan, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setPan: {e}")))
}

fn set_rate_impl<'js>(ctx: Ctx<'js>, id: u64, rate: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_rate(&ctx, "setRate", rate)?;
  let ramp_ms = read_ramp_ms(&ctx, "setRate", &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.set_audio_rate(id, rate, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setRate: {e}")))
}

fn set_master_gain_impl<'js>(ctx: Ctx<'js>, gain: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_gain(&ctx, "setMasterGain", gain)?;
  let ramp_ms = read_ramp_ms(&ctx, "setMasterGain", &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.set_master_gain(gain, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setMasterGain: {e}")))
}

/// Declared contract, not yet implemented (lands with the own-mixer
/// replacement; over SDL_mixer a real bus gain layer would need disposable
/// engine-side composition - see okf/backlog/flux-audio-mix-control.md 3b).
/// Throws so the unimplemented state cannot be missed; the .d.ts documents
/// the interim pattern.
fn set_bus_gain_impl(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  Err(throw_str(&ctx, "setBusGain: not implemented yet; keep a bus gain in the app and multiply it into each voice's setGain"))
}

fn output_sample_rate_impl(ctx: Ctx<'_>) -> rquickjs::Result<i32> {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.audio_output_sample_rate().map_err(|e| throw_str(&ctx, &format!("outputSampleRate: {e}")))
}

fn ended_impl(ctx: Ctx<'_>, id: u64) -> bool {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.audio_ended(id)
}

fn unload_impl(ctx: Ctx<'_>, sound_id: u64) {
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.unload_sound(sound_id);
}

fn stop_impl<'js>(ctx: Ctx<'js>, id: u64, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let fade_out_ms = read_fade_out_ms(&ctx, "stop", &options)?;
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  state.0.stop_audio(id, fade_out_ms);
  Ok(())
}

fn stop_all_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let fade_out_ms = read_fade_out_ms(&ctx, "stop", &options)?;
  let bus = match &options.0 {
    Some(opts) => opts.get::<_, Option<String>>("bus")?,
    None => None,
  };
  let state = ctx.userdata::<AudioPluginState>().expect("audio state");
  match bus {
    Some(bus) => {
      check_bus(&ctx, "stop", &bus)?;
      state.0.stop_bus_audio(&bus, fade_out_ms);
    }
    None => state.0.stop_all_audio(fade_out_ms),
  }
  Ok(())
}
