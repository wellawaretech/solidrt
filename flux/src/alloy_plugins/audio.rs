//! JS bindings for sound playback: a thin marshaling layer over alloy::audio
//! (the mixer) and forge::audio (the decoders). Loading (`load`, `loadPcm`,
//! `stream`) yields a clip handle; starting a clip (or the fire-and-forget
//! `play`) yields a playback handle with live controls; `createSink` yields
//! a sink handle for samples pushed as they are produced; `createListener`
//! yields the listener a playback is placed against when it is attached to
//! a spatial node (`playback.attach`, the `emitter` play option: the
//! spatial core then writes its pan, gain, rate and low-pass). Raw sound,
//! track, sink and listener ids stay in Rust.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use alloy::audio::{AudioTrackReader, ClipFormat, ClipSource, Pcm, SpatialParams};
use alloy::spatial::{DistanceModel, EmitterParams, EmitterSource, ListenerSource, SPEED_OF_SOUND};
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
  /// The audio listeners this engine created and has not destroyed: what
  /// its end destroys (the emitters on them go with their voices).
  listeners: RefCell<HashSet<u64>>,
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
    for listener in self.listeners.get_mut().drain() {
      // Already gone when the spatial plugin reset the arena first.
      let _ = alloy.spatial().destroy_listener(listener);
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
      listeners: RefCell::new(HashSet::new()),
      master_gain_set: Cell::new(false),
    })))
    .expect("store audio state");
}

fn state(ctx: &Ctx<'_>) -> Rc<Inner> {
  ctx.userdata::<AudioPluginState>().expect("audio state").0.clone()
}

/// The `flux:audio` module. Handles are bound objects (playback: `{ stop,
/// pause, resume, setGain, setPan, setRate, setLoop, attach, detach,
/// position, ended }`; clip: `{ duration, play, unload }`; sink: `{ id,
/// push, queuedMs, positionMs, pause, resume, clear, setGain, close }`;
/// listener: `{ id, set, destroy }`) so raw ids stay in Rust.
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
    decl.declare("createListener")?;
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
    exports.export("createListener", Function::new(ctx.clone(), create_listener_impl)?)?;
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

/// And for a capturing closure taking an object (an emitter, a listener
/// source) and an options bag.
fn object_arg_builder<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, Object<'js>, OptArg<Object<'js>>) -> rquickjs::Result<()>,
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
  if parsed.pan.is_some() && emitter_of(options)?.is_some() {
    return Err(throw_str(ctx, "play: pan and emitter are exclusive (the emitter owns the pan)"));
  }
  Ok(parsed)
}

/// The `emitter` play option, when given.
fn emitter_of<'js>(options: &OptArg<Object<'js>>) -> rquickjs::Result<Option<Object<'js>>> {
  match &options.0 {
    Some(opts) => opts.get::<_, Option<Object<'js>>>("emitter"),
    None => Ok(None),
  }
}

/// A listener source `{ node }` or `{ view }` (exactly one).
fn read_listener_source(ctx: &Ctx<'_>, who: &str, source: &Object<'_>) -> rquickjs::Result<ListenerSource> {
  let node = source.get::<_, Option<f64>>("node")?;
  let view = source.get::<_, Option<f64>>("view")?;
  match (node, view) {
    (Some(n), None) => Ok(ListenerSource::Node(check_id(ctx, who, "node", n)?)),
    (None, Some(v)) => Ok(ListenerSource::View(check_id(ctx, who, "view", v)?)),
    _ => Err(throw_str(ctx, &format!("{who}: a listener source is {{ node }} or {{ view }}, exactly one"))),
  }
}

/// A node, target or view id: a non-negative integer in the range of the
/// ids the runtime hands out.
fn check_id(ctx: &Ctx<'_>, who: &str, what: &str, id: f64) -> rquickjs::Result<u64> {
  if !(id.is_finite() && id >= 0.0 && id.fract() == 0.0 && id <= u64::MAX as f64) {
    return Err(throw_str(ctx, &format!("{who}: {what} must be an id (a non-negative integer)")));
  }
  Ok(id as u64)
}

/// The `{ speedOfSound? }` listener options.
fn read_speed_of_sound(ctx: &Ctx<'_>, who: &str, options: &OptArg<Object<'_>>) -> rquickjs::Result<f64> {
  let Some(opts) = &options.0 else { return Ok(SPEED_OF_SOUND) };
  let speed = opts.get::<_, Option<f64>>("speedOfSound")?.unwrap_or(SPEED_OF_SOUND);
  if !(speed.is_finite() && speed > 0.0) {
    return Err(throw_str(ctx, &format!("{who}: speedOfSound must be a finite number > 0")));
  }
  Ok(speed)
}

/// An emitter `{ node | position, listener, distanceModel?, refDistance?,
/// maxDistance?, rolloff?, doppler? }`: the source, the listener's id and
/// the parameters (the ranges are the core's to check at bind).
fn read_emitter(
  ctx: &Ctx<'_>,
  who: &str,
  emitter: &Object<'_>,
) -> rquickjs::Result<(EmitterSource, u64, EmitterParams)> {
  let node = emitter.get::<_, Option<f64>>("node")?;
  let position = emitter.get::<_, Option<Vec<f64>>>("position")?;
  let source = match (node, position) {
    (Some(n), None) => EmitterSource::Node(check_id(ctx, who, "emitter.node", n)?),
    (None, Some(p)) if p.len() == 3 => EmitterSource::Fixed([p[0], p[1], p[2]]),
    (None, Some(_)) => return Err(throw_str(ctx, &format!("{who}: emitter.position must be [x, y, z]"))),
    _ => return Err(throw_str(ctx, &format!("{who}: an emitter names a node or a position, exactly one"))),
  };
  let listener = emitter
    .get::<_, Option<Object<'_>>>("listener")?
    .and_then(|l| l.get::<_, Option<f64>>("id").ok().flatten())
    .ok_or_else(|| throw_str(ctx, &format!("{who}: emitter.listener must be a Listener (createListener)")))?;
  let listener = check_id(ctx, who, "emitter.listener", listener)?;
  let defaults = EmitterParams::default();
  let model = match emitter.get::<_, Option<String>>("distanceModel")?.as_deref() {
    None | Some("inverse") => DistanceModel::Inverse,
    Some("linear") => DistanceModel::Linear,
    Some("exponential") => DistanceModel::Exponential,
    Some(other) => {
      return Err(throw_str(
        ctx,
        &format!("{who}: distanceModel must be \"inverse\", \"linear\" or \"exponential\", got \"{other}\""),
      ))
    }
  };
  let params = EmitterParams {
    ref_distance: emitter.get::<_, Option<f64>>("refDistance")?.unwrap_or(defaults.ref_distance),
    max_distance: emitter.get::<_, Option<f64>>("maxDistance")?.unwrap_or(defaults.max_distance),
    rolloff: emitter.get::<_, Option<f64>>("rolloff")?.unwrap_or(defaults.rolloff),
    model,
    doppler: emitter.get::<_, Option<f64>>("doppler")?.unwrap_or(defaults.doppler),
  };
  Ok((source, listener, params))
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
  obj.set(
    "attach",
    Function::new(ctx.clone(), object_arg_builder(move |ctx, emitter, _| attach_impl(ctx, track_id, emitter)))?,
  )?;
  obj.set(
    "detach",
    Function::new(ctx.clone(), stop_builder(move |ctx, options| detach_impl(ctx, track_id, options)))?,
  )?;
  let position_reader = reader.clone();
  obj.set("position", Function::new(ctx.clone(), move || position_reader.position())?)?;
  obj.set("ended", Function::new(ctx.clone(), move || reader.ended())?)?;
  Ok(obj)
}

/// Start a voice, note it, place it if the options name an emitter (the
/// placement lands in the same mix block as the start, so the first sample
/// is already placed), and wrap it.
fn started<'js>(
  ctx: &Ctx<'js>,
  state: &Inner,
  id: u64,
  options: &OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  state.track_started(id);
  let reader = state.gui.alloy.audio_track_reader(id).ok_or_else(|| throw_str(ctx, "play: track vanished"))?;
  if let Some(emitter) = emitter_of(options)? {
    if let Err(e) = attach(ctx, "play", id, &emitter) {
      // A refused emitter must not leave a plain voice playing at full level.
      state.gui.alloy.stop_audio(id, 0.0);
      state.tracks.borrow_mut().remove(&id);
      return Err(e);
    }
  }
  playback_handle(ctx, id, reader)
}

/// Bind a track to an emitter and place it at once (no ramp): the core
/// evaluates the binding against the tree as it stands and the mixer
/// applies it in its next block. A track that ended is left alone.
fn attach(ctx: &Ctx<'_>, who: &str, track_id: u64, emitter: &Object<'_>) -> rquickjs::Result<()> {
  let (source, listener, params) = read_emitter(ctx, who, emitter)?;
  let alloy = &super::gui(ctx).alloy;
  if alloy.audio_ended(track_id) {
    return Ok(());
  }
  let output = alloy
    .spatial()
    .bind_emitter(track_id, source, listener, params)
    .map_err(|e| throw_str(ctx, &format!("{who}: {e}")))?;
  let placed = SpatialParams { pan: output.pan, gain: output.gain, rate: output.rate, cutoff: output.cutoff };
  let landed = alloy.set_audio_spatial(track_id, placed, 0.0).map_err(|e| throw_str(ctx, &format!("{who}: {e}")))?;
  if !landed {
    alloy.spatial().unbind_emitter(track_id);
  }
  Ok(())
}

/// playback.attach(emitter): place the playback at a node or position,
/// heard by a listener (see `read_emitter`); attaching again updates.
fn attach_impl<'js>(ctx: Ctx<'js>, track_id: u64, emitter: Object<'js>) -> rquickjs::Result<()> {
  attach(&ctx, "attach", track_id, &emitter)
}

/// playback.detach({ rampMs? }): the placement's gain and rate layers ramp
/// back to 1 and the low-pass opens; the pan stays where the emitter left
/// it, and setPan is accepted again.
fn detach_impl<'js>(ctx: Ctx<'js>, track_id: u64, options: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let ramp_ms = read_ramp_ms(&ctx, "detach", &options)?;
  let alloy = &super::gui(&ctx).alloy;
  alloy.spatial().unbind_emitter(track_id);
  alloy.clear_audio_spatial(track_id, ramp_ms).map_err(|e| throw_str(&ctx, &format!("detach: {e}")))
}

/// createListener(source, { speedOfSound? }) -> listener: what emitters
/// are heard by, a node's position and axes or a target's view (the
/// camera). Several may exist, one per scene.
fn create_listener_impl<'js>(
  ctx: Ctx<'js>,
  source: Object<'js>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let listener_source = read_listener_source(&ctx, "createListener", &source)?;
  let speed = read_speed_of_sound(&ctx, "createListener", &options)?;
  let state = state(&ctx);
  let id = state
    .gui
    .alloy
    .spatial()
    .create_listener(listener_source, speed)
    .map_err(|e| throw_str(&ctx, &format!("createListener: {e}")))?;
  state.listeners.borrow_mut().insert(id);
  listener_handle(&ctx, id)
}

/// The bound listener object: `{ id, set, destroy }`.
fn listener_handle<'js>(ctx: &Ctx<'js>, id: u64) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("id", id as f64)?;
  obj.set(
    "set",
    Function::new(
      ctx.clone(),
      object_arg_builder(move |ctx, source, options| listener_set_impl(ctx, id, source, options)),
    )?,
  )?;
  obj.set("destroy", Function::new(ctx.clone(), move |ctx: Ctx<'_>| listener_destroy_impl(ctx, id))?)?;
  Ok(obj)
}

/// listener.set(source, { speedOfSound? }): re-point the listener (a camera
/// switch, a node taking over); its emitters re-place at the next frame.
fn listener_set_impl<'js>(
  ctx: Ctx<'js>,
  id: u64,
  source: Object<'js>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<()> {
  let listener_source = read_listener_source(&ctx, "listener.set", &source)?;
  let speed = read_speed_of_sound(&ctx, "listener.set", &options)?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_listener(id, listener_source, speed)
    .map_err(|e| throw_str(&ctx, &format!("listener.set: {e}")))
}

/// listener.destroy(): the emitters heard by it are released (their
/// playbacks keep the last placement); idempotent.
fn listener_destroy_impl(ctx: Ctx<'_>, id: u64) {
  let state = state(&ctx);
  let _ = state.gui.alloy.spatial().destroy_listener(id);
  state.listeners.borrow_mut().remove(&id);
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
  started(&ctx, &state, id, &options)
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
  started(&ctx, &state, id, &options)
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
