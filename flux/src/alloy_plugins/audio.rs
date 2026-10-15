//! JS bindings for sound playback: a thin marshaling layer over alloy::audio
//! (the mixer) and forge::audio (the decoders). Loading (`load`, `loadPcm`,
//! `stream`) yields a clip handle; starting a clip (or the fire-and-forget
//! `play`) yields a playback handle with live controls; `createSink` yields
//! a sink handle for samples pushed as they are produced. Raw sound, track
//! and sink ids stay in Rust.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use alloy::audio::{AudioTrackReader, ClipFormat, ClipSource, Pcm};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, FromJs, Function, JsLifetime, Object, TypedArray, Value};

use crate::plugins::marshal::{bytes_of, elements_of, OptArg};
use crate::plugins::seekable::SeekableSource;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

// The gain the master and every bus start at, restored at the end of an
// engine that changed them: the device outlives apps, and the next must
// not inherit a mute or a paused bus.
const DEFAULT_GAIN: f32 = 1.0;

/// forge's decoded clip as alloy's: the same fields, two crates.
fn to_alloy(pcm: forge::audio::Pcm) -> Pcm {
  Pcm { sample_rate: pcm.sample_rate, channels: pcm.channels, samples: pcm.samples }
}

/// forge's clip decoder under alloy's streaming contract: pure forwarding,
/// the marshalling between the two crates (alloy knows no format).
struct ClipAdapter(Box<dyn forge::audio::AudioDecoder>);

impl ClipSource for ClipAdapter {
  fn format(&self) -> ClipFormat {
    let format = self.0.format();
    ClipFormat { sample_rate: format.sample_rate, channels: format.channels, frames: format.frames }
  }
  fn read(&mut self, out: &mut [f32]) -> Result<usize, String> {
    self.0.read(out)
  }
  fn seek(&mut self, frame: u64) -> Result<(), String> {
    self.0.seek(frame)
  }
}

struct Inner {
  gui: Rc<super::Gui>,
  /// The tracks this engine started and may still be playing: what its
  /// end stops. Pruned of finished ones at each start (alloy sweeps them
  /// from its registry; a stale id here would only hit a no-op).
  tracks: RefCell<HashSet<u64>>,
  /// The clips this engine loaded and has not unloaded: what its end frees.
  sounds: RefCell<HashSet<u64>>,
  /// The sinks this engine opened and has not closed: what its end closes.
  sinks: RefCell<HashSet<u64>>,
  /// The buses this engine set a gain or pause on, which its end resets.
  buses: RefCell<HashSet<String>>,
  /// Whether the app set the device's master gain, which its end restores.
  master_gain_set: Cell<bool>,
}

impl Drop for Inner {
  // Engine teardown: the app's handles died with it, so nothing else will
  // ever stop its voices, close its sinks, free its clips or undo its
  // gains. The device itself stays open. (Video's sinks are the video
  // plugin's.)
  fn drop(&mut self) {
    let alloy = &self.gui.alloy;
    for track in self.tracks.get_mut().drain() {
      alloy.stop_audio(track, 0.0);
    }
    for sink in self.sinks.get_mut().drain() {
      alloy.destroy_pcm_sink(sink);
    }
    for sound in self.sounds.get_mut().drain() {
      alloy.unload_sound(sound);
    }
    for bus in self.buses.get_mut().drain() {
      if let Err(e) = alloy.set_bus_gain(&bus, DEFAULT_GAIN, 0.0).and_then(|()| alloy.set_bus_paused(&bus, false)) {
        log::warn!("[audio] reset bus {bus}: {e}");
      }
    }
    if self.master_gain_set.get() {
      if let Err(e) = alloy.set_master_gain(DEFAULT_GAIN, 0.0) {
        log::warn!("[audio] restore master gain: {e}");
      }
    }
  }
}

impl Inner {
  // Note a started track, letting go of the ones that finished since.
  fn track_started(&self, id: u64) {
    let mut tracks = self.tracks.borrow_mut();
    tracks.retain(|track| !self.gui.alloy.audio_ended(*track));
    tracks.insert(id);
  }
}

#[derive(Clone, JsLifetime)]
struct AudioPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the audio plugin state in userdata. Runs at engine init, before any
/// module import; the `flux:audio` module surface reads it.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx
    .store_userdata(AudioPluginState(Rc::new(Inner {
      gui: super::gui(ctx),
      tracks: RefCell::new(HashSet::new()),
      sounds: RefCell::new(HashSet::new()),
      sinks: RefCell::new(HashSet::new()),
      buses: RefCell::new(HashSet::new()),
      master_gain_set: Cell::new(false),
    })))
    .expect("store audio state");
}

fn state(ctx: &Ctx<'_>) -> Rc<Inner> {
  ctx.userdata::<AudioPluginState>().expect("audio state").0.clone()
}

/// The `flux:audio` module. Handles are bound objects (playback: `{ stop,
/// pause, resume, setGain, setPan, setRate, setLoop, position, ended }`;
/// clip: `{ duration, play, unload }`; sink: `{ id, push, queuedMs,
/// positionMs, pause, resume, clear, setGain, close }`) so raw ids stay in
/// Rust.
pub struct AudioModule;

impl ModuleDef for AudioModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("play")?;
    decl.declare("load")?;
    decl.declare("loadPcm")?;
    decl.declare("stream")?;
    decl.declare("stop")?;
    decl.declare("pause")?;
    decl.declare("resume")?;
    decl.declare("setMasterGain")?;
    decl.declare("setBusGain")?;
    decl.declare("createSink")?;
    decl.declare("outputSampleRate")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("play", Function::new(ctx.clone(), play_impl)?)?;
    exports.export("load", Function::new(ctx.clone(), load_impl)?)?;
    exports.export("loadPcm", Function::new(ctx.clone(), load_pcm_impl)?)?;
    exports.export("stream", Function::new(ctx.clone(), stream_impl)?)?;
    exports.export("stop", Function::new(ctx.clone(), stop_all_impl)?)?;
    exports.export("pause", Function::new(ctx.clone(), pause_all_impl)?)?;
    exports.export("resume", Function::new(ctx.clone(), resume_all_impl)?)?;
    exports.export("setMasterGain", Function::new(ctx.clone(), set_master_gain_impl)?)?;
    exports.export("setBusGain", Function::new(ctx.clone(), set_bus_gain_impl)?)?;
    exports.export("createSink", Function::new(ctx.clone(), create_sink_impl)?)?;
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

/// And for a capturing closure taking a typed array.
fn push_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, TypedArray<'js, f32>) -> rquickjs::Result<()>,
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

/// Read the `{ bus? }` option the module-level group calls share.
fn read_bus(ctx: &Ctx<'_>, who: &str, options: &OptArg<Object<'_>>) -> rquickjs::Result<Option<String>> {
  let Some(opts) = &options.0 else { return Ok(None) };
  let bus = opts.get::<_, Option<String>>("bus")?;
  if let Some(bus) = &bus {
    check_bus(ctx, who, bus)?;
  }
  Ok(bus)
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

fn check_sample_rate(ctx: &Ctx<'_>, who: &str, sample_rate: f64) -> rquickjs::Result<u32> {
  if !sample_rate.is_finite() || sample_rate < 1.0 || sample_rate > i32::MAX as f64 || sample_rate.fract() != 0.0 {
    return Err(throw_str(ctx, &format!("{who}: sampleRate must be a positive integer")));
  }
  Ok(sample_rate as u32)
}

fn check_channels(ctx: &Ctx<'_>, who: &str, channels: i32) -> rquickjs::Result<u16> {
  if channels != 1 && channels != 2 {
    return Err(throw_str(ctx, &format!("{who}: channels must be 1 or 2")));
  }
  Ok(channels as u16)
}

/// Wrap a started track in a playback handle (`{ stop, pause, resume,
/// setGain, setPan, setRate, setLoop, position, ended }`) so the raw id
/// stays in Rust. `reader` answers position and ended after the registry
/// has forgotten the track.
fn playback_handle<'js>(ctx: &Ctx<'js>, track_id: u64, reader: AudioTrackReader) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("stop", Function::new(ctx.clone(), stop_builder(move |ctx, options| stop_impl(ctx, track_id, options)))?)?;
  obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| set_paused_impl(ctx, track_id, true))?)?;
  obj.set("resume", Function::new(ctx.clone(), move |ctx: Ctx<'_>| set_paused_impl(ctx, track_id, false))?)?;
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
  obj.set(
    "setLoop",
    Function::new(ctx.clone(), move |ctx: Ctx<'_>, looping: bool| set_loop_impl(ctx, track_id, looping))?,
  )?;
  let position_reader = reader.clone();
  obj.set("position", Function::new(ctx.clone(), move || position_reader.position())?)?;
  obj.set("ended", Function::new(ctx.clone(), move || reader.ended())?)?;
  Ok(obj)
}

/// Start a voice, note it, and wrap it.
fn started<'js>(ctx: &Ctx<'js>, state: &Inner, id: u64) -> rquickjs::Result<Object<'js>> {
  state.track_started(id);
  let reader = state.gui.alloy.audio_track_reader(id).ok_or_else(|| throw_str(ctx, "play: track vanished"))?;
  playback_handle(ctx, id, reader)
}

/// play(bytes, { loop?, gain?, pan?, rate?, fadeInMs?, bus? }) -> playback.
/// Fire-and-forget: decodes and starts in one call. Use `load` to replay a
/// clip without re-decoding.
fn play_impl<'js>(
  ctx: Ctx<'js>,
  data: TypedArray<'js, u8>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let play_options = read_options(&ctx, &options)?;
  let bytes = bytes_of(&ctx, &data, "play")?;
  let pcm = forge::audio::decode(bytes).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  let state = state(&ctx);
  let id =
    state.gui.alloy.play_audio(to_alloy(pcm), &play_options).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  started(&ctx, &state, id)
}

/// Wrap a loaded sound id in a clip handle (`{ duration, play, unload }`),
/// keeping the id in Rust.
fn clip_handle<'js>(ctx: &Ctx<'js>, sound_id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  // None (undefined in JS) for a stream whose container states no length.
  obj.set("duration", state(ctx).gui.alloy.sound_duration(sound_id))?;
  let play_fn =
    Function::new(ctx.clone(), object_builder(move |ctx, options| play_sound_impl(ctx, sound_id, options)))?;
  obj.set("play", play_fn)?;
  let unload_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| unload_impl(ctx, sound_id))?;
  obj.set("unload", unload_fn)?;
  Ok(obj)
}

/// load(bytes) -> clip. Decodes the clip (WAV, Opus in Ogg or WebM) once;
/// each `play` starts a fresh overlapping playback with no decode.
fn load_impl<'js>(ctx: Ctx<'js>, data: TypedArray<'js, u8>) -> rquickjs::Result<Object<'js>> {
  let bytes = bytes_of(&ctx, &data, "load")?;
  let pcm = forge::audio::decode(bytes).map_err(|e| throw_str(&ctx, &format!("load: {e}")))?;
  let state = state(&ctx);
  let sound_id = state.gui.alloy.load_sound(to_alloy(pcm)).map_err(|e| throw_str(&ctx, &format!("load: {e}")))?;
  state.sounds.borrow_mut().insert(sound_id);
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
  let sample_rate = check_sample_rate(&ctx, "loadPcm", sample_rate)?;
  let channels = match &options.0 {
    Some(opts) => opts.get::<_, Option<i32>>("channels")?.unwrap_or(1),
    None => 1,
  };
  let channels = check_channels(&ctx, "loadPcm", channels)?;
  let samples = match &data {
    PcmData::U8(a) => forge::audio::samples_from_u8(elements_of(&ctx, a, "loadPcm")?),
    PcmData::S16(a) => forge::audio::samples_from_i16(elements_of(&ctx, a, "loadPcm")?),
    PcmData::F32(a) => elements_of(&ctx, a, "loadPcm")?.to_vec(),
  };
  let pcm = Pcm { sample_rate, channels, samples };
  let state = state(&ctx);
  let sound_id = state.gui.alloy.load_sound(pcm).map_err(|e| throw_str(&ctx, &format!("loadPcm: {e}")))?;
  state.sounds.borrow_mut().insert(sound_id);
  clip_handle(&ctx, sound_id)
}

/// stream(source) -> clip. `source` is a `file()` from `flux:fs`; its native
/// seekable source is opened by forge's decoder for on-demand decoding
/// (large tracks stay off the heap) on the mixer's decode thread. Taking
/// the file object rather than a path means streaming rides the file's
/// attached backend: a packed asset streams via range reads out of the exe
/// instead of a plain disk file. A stream plays one voice at a time.
fn stream_impl<'js>(ctx: Ctx<'js>, source: Object<'js>) -> rquickjs::Result<Object<'js>> {
  let reader = SeekableSource::open_from(&source).map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  let decoder = forge::audio::open(reader).map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  let state = state(&ctx);
  let sound_id = state
    .gui
    .alloy
    .stream_sound(Box::new(ClipAdapter(decoder)))
    .map_err(|e| throw_str(&ctx, &format!("stream: {e}")))?;
  state.sounds.borrow_mut().insert(sound_id);
  clip_handle(&ctx, sound_id)
}

fn play_sound_impl<'js>(ctx: Ctx<'js>, sound_id: u64, options: OptArg<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let play_options = read_options(&ctx, &options)?;
  let state = state(&ctx);
  let id = state.gui.alloy.play_sound(sound_id, &play_options).map_err(|e| throw_str(&ctx, &format!("play: {e}")))?;
  started(&ctx, &state, id)
}

fn set_gain_impl<'js>(ctx: Ctx<'js>, id: u64, gain: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_gain(&ctx, "setGain", gain)?;
  let ramp_ms = read_ramp_ms(&ctx, "setGain", &options)?;
  let gui = super::gui(&ctx);
  gui.alloy.set_audio_gain(id, gain, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setGain: {e}")))
}

fn set_pan_impl<'js>(ctx: Ctx<'js>, id: u64, pan: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_pan(&ctx, "setPan", pan)?;
  let ramp_ms = read_ramp_ms(&ctx, "setPan", &options)?;
  let gui = super::gui(&ctx);
  gui.alloy.set_audio_pan(id, pan, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setPan: {e}")))
}

fn set_rate_impl<'js>(ctx: Ctx<'js>, id: u64, rate: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_rate(&ctx, "setRate", rate)?;
  let ramp_ms = read_ramp_ms(&ctx, "setRate", &options)?;
  let gui = super::gui(&ctx);
  gui.alloy.set_audio_rate(id, rate, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setRate: {e}")))
}

fn set_loop_impl(ctx: Ctx<'_>, id: u64, looping: bool) {
  super::gui(&ctx).alloy.set_audio_loop(id, looping);
}

fn set_paused_impl(ctx: Ctx<'_>, id: u64, paused: bool) {
  super::gui(&ctx).alloy.set_audio_paused(id, paused);
}

fn set_master_gain_impl<'js>(ctx: Ctx<'js>, gain: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_gain(&ctx, "setMasterGain", gain)?;
  let ramp_ms = read_ramp_ms(&ctx, "setMasterGain", &options)?;
  let state = state(&ctx);
  state.master_gain_set.set(true);
  state.gui.alloy.set_master_gain(gain, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setMasterGain: {e}")))
}

/// setBusGain(bus, gain, { rampMs? }): a bus's own gain layer, under the
/// master and over each voice's.
fn set_bus_gain_impl<'js>(ctx: Ctx<'js>, bus: String, gain: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_bus(&ctx, "setBusGain", &bus)?;
  check_gain(&ctx, "setBusGain", gain)?;
  let ramp_ms = read_ramp_ms(&ctx, "setBusGain", &options)?;
  let state = state(&ctx);
  state.gui.alloy.set_bus_gain(&bus, gain, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setBusGain: {e}")))?;
  state.buses.borrow_mut().insert(bus);
  Ok(())
}

fn output_sample_rate_impl(ctx: Ctx<'_>) -> rquickjs::Result<i32> {
  let gui = super::gui(&ctx);
  gui.alloy.audio_output_sample_rate().map_err(|e| throw_str(&ctx, &format!("outputSampleRate: {e}")))
}

fn unload_impl(ctx: Ctx<'_>, sound_id: u64) {
  let state = state(&ctx);
  state.gui.alloy.unload_sound(sound_id);
  state.sounds.borrow_mut().remove(&sound_id);
}

fn stop_impl<'js>(ctx: Ctx<'js>, id: u64, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let fade_out_ms = read_fade_out_ms(&ctx, "stop", &options)?;
  let state = state(&ctx);
  state.gui.alloy.stop_audio(id, fade_out_ms);
  // A fading track plays on until the fade ends; the next start prunes it.
  if fade_out_ms == 0.0 {
    state.tracks.borrow_mut().remove(&id);
  }
  Ok(())
}

fn stop_all_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let fade_out_ms = read_fade_out_ms(&ctx, "stop", &options)?;
  let bus = read_bus(&ctx, "stop", &options)?;
  let state = state(&ctx);
  match bus {
    Some(bus) => state.gui.alloy.stop_bus_audio(&bus, fade_out_ms),
    None => {
      state.gui.alloy.stop_all_audio(fade_out_ms);
      if fade_out_ms == 0.0 {
        state.tracks.borrow_mut().clear();
      }
    }
  }
  Ok(())
}

/// pause({ bus? }) / resume({ bus? }): hold or continue every playback, or
/// one bus (a bus pause holds playbacks started on it later too).
fn set_all_paused<'js>(ctx: Ctx<'js>, who: &str, paused: bool, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let bus = read_bus(&ctx, who, &options)?;
  let state = state(&ctx);
  match bus {
    Some(bus) => {
      state.gui.alloy.set_bus_paused(&bus, paused).map_err(|e| throw_str(&ctx, &format!("{who}: {e}")))?;
      state.buses.borrow_mut().insert(bus);
    }
    None => state.gui.alloy.pause_all_audio(paused),
  }
  Ok(())
}

fn pause_all_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  set_all_paused(ctx, "pause", true, options)
}

fn resume_all_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  set_all_paused(ctx, "resume", false, options)
}

/// createSink({ sampleRate, channels?, bus? }) -> sink. A streaming PCM
/// output on the mixer; its `id` is what a runtime module takes a pushing
/// handle by.
fn create_sink_impl<'js>(ctx: Ctx<'js>, options: Object<'js>) -> rquickjs::Result<Object<'js>> {
  let sample_rate = options
    .get::<_, Option<f64>>("sampleRate")?
    .ok_or_else(|| throw_str(&ctx, "createSink: sampleRate is required"))?;
  let sample_rate = check_sample_rate(&ctx, "createSink", sample_rate)?;
  let channels = check_channels(&ctx, "createSink", options.get::<_, Option<i32>>("channels")?.unwrap_or(1))?;
  let bus = options.get::<_, Option<String>>("bus")?;
  if let Some(bus) = &bus {
    check_bus(&ctx, "createSink", bus)?;
  }
  let state = state(&ctx);
  let id = state
    .gui
    .alloy
    .create_pcm_sink(sample_rate, channels, bus.as_deref())
    .map_err(|e| throw_str(&ctx, &format!("createSink: {e}")))?;
  state.sinks.borrow_mut().insert(id);
  sink_handle(&ctx, id)
}

/// The bound sink object: `{ id, push, queuedMs, positionMs, pause, resume,
/// clear, setGain, close }`.
fn sink_handle<'js>(ctx: &Ctx<'js>, id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("id", id)?;
  obj.set("push", Function::new(ctx.clone(), push_builder(move |ctx, samples| sink_push_impl(ctx, id, samples)))?)?;
  obj.set("queuedMs", Function::new(ctx.clone(), move |ctx: Ctx<'_>| sink_queued_ms_impl(ctx, id))?)?;
  obj.set("positionMs", Function::new(ctx.clone(), move |ctx: Ctx<'_>| sink_position_ms_impl(ctx, id))?)?;
  obj.set("pause", Function::new(ctx.clone(), move |ctx: Ctx<'_>| sink_set_paused_impl(ctx, id, true))?)?;
  obj.set("resume", Function::new(ctx.clone(), move |ctx: Ctx<'_>| sink_set_paused_impl(ctx, id, false))?)?;
  obj.set("clear", Function::new(ctx.clone(), move |ctx: Ctx<'_>| sink_clear_impl(ctx, id))?)?;
  obj.set(
    "setGain",
    Function::new(ctx.clone(), setter_builder(move |ctx, gain, options| sink_set_gain_impl(ctx, id, gain, options)))?,
  )?;
  obj.set("close", Function::new(ctx.clone(), move |ctx: Ctx<'_>| sink_close_impl(ctx, id))?)?;
  Ok(obj)
}

fn sink_push_impl<'js>(ctx: Ctx<'js>, id: u64, samples: TypedArray<'js, f32>) -> rquickjs::Result<()> {
  let samples = elements_of(&ctx, &samples, "push")?;
  super::gui(&ctx).alloy.pcm_sink_push(id, samples).map_err(|e| throw_str(&ctx, &format!("push: {e}")))
}

fn sink_queued_ms_impl(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<f64> {
  let us = super::gui(&ctx).alloy.pcm_sink_queued_us(id).map_err(|e| throw_str(&ctx, &format!("queuedMs: {e}")))?;
  Ok(us as f64 / 1000.0)
}

fn sink_position_ms_impl(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<f64> {
  let us = super::gui(&ctx).alloy.pcm_sink_position_us(id).map_err(|e| throw_str(&ctx, &format!("positionMs: {e}")))?;
  Ok(us as f64 / 1000.0)
}

fn sink_set_paused_impl(ctx: Ctx<'_>, id: u64, paused: bool) -> rquickjs::Result<()> {
  let who = if paused { "pause" } else { "resume" };
  super::gui(&ctx).alloy.set_pcm_sink_paused(id, paused).map_err(|e| throw_str(&ctx, &format!("{who}: {e}")))
}

fn sink_clear_impl(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.clear_pcm_sink(id).map_err(|e| throw_str(&ctx, &format!("clear: {e}")))
}

fn sink_set_gain_impl<'js>(ctx: Ctx<'js>, id: u64, gain: f32, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  check_gain(&ctx, "setGain", gain)?;
  let ramp_ms = read_ramp_ms(&ctx, "setGain", &options)?;
  super::gui(&ctx).alloy.set_pcm_sink_gain(id, gain, ramp_ms).map_err(|e| throw_str(&ctx, &format!("setGain: {e}")))
}

fn sink_close_impl(ctx: Ctx<'_>, id: u64) {
  let state = state(&ctx);
  state.gui.alloy.destroy_pcm_sink(id);
  state.sinks.borrow_mut().remove(&id);
}
