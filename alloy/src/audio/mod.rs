//! Sound playback: one mixer for clips, streamed tracks, PCM sinks and
//! video audio, rendered in SDL's device callback with sample-accurate
//! automation (okf/plans/audio-unification.md).
//!
//! The pieces: `engine` is the realtime core (the voice table, buses, the
//! master, ramps, pan, the mix), SDL-free; `device` is the SDL device
//! stream whose get callback runs the engine, and the SDL converter each
//! voice resamples through; `ring` carries frames from a producer thread
//! to the callback; `stream` is the decode thread filling streaming clips'
//! rings. This module is the registry the control thread (UI/JS) talks to:
//! it owns clips, tracks, sinks and bus names, hands out ids, and turns
//! calls into engine commands. Everything here is touched only from the
//! UI/JS thread, like the microphone; the callback and the decode thread
//! reach shared state through atomics and the command queue.
//!
//! Decoding is not here: alloy takes decoded `Pcm` and `ClipSource`s (the
//! host decodes with forge::audio), so this module knows no format.

mod device;
pub(crate) mod engine;
pub(crate) mod ring;
mod stream;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use engine::{clamp_rate, pcm_all_finite, Command, Ramp, Slot, Source, Voice, SLOT_TABLE};
pub use engine::{Pcm, VoiceShared, MAX_BUSES, MAX_LIVE_TRACKS};

use device::Device;
use ring::{RingConsumer, RingProducer, RingShared};
use stream::{Decoder, StreamControl};

/// A sink ring's capacity in seconds of its rate: the video worker keeps
/// half a second queued, a JS pusher is told the same; room for a few of
/// those between blocks.
const SINK_RING_SECONDS: f64 = 2.0;

/// What a streaming clip's source says about itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipFormat {
  pub sample_rate: u32,
  pub channels: u16,
  /// The length in frames when the container says.
  pub frames: Option<u64>,
}

/// A clip decoded on request from the decode thread: the host's decoder
/// (forge::audio's) behind alloy's own contract, so alloy knows no format.
pub trait ClipSource: Send {
  fn format(&self) -> ClipFormat;
  /// Decode whole frames into `out` (interleaved); the frames written, 0 at
  /// the end.
  fn read(&mut self, out: &mut [f32]) -> Result<usize, String>;
  /// Reposition so the next read starts at `frame`.
  fn seek(&mut self, frame: u64) -> Result<(), String>;
}

/// Options for starting a voice. `gain` defaults to 1.0 (as loaded);
/// `pan`/`rate` of None leave the voice unspatialized / at the loaded rate;
/// `fade_in_ms` > 0 fades in from silence; `bus` names the group the voice
/// belongs to (created by use), which has its own gain and pause.
pub struct PlayOptions {
  pub looping: bool,
  pub gain: f32,
  pub pan: Option<f32>,
  pub rate: Option<f32>,
  pub fade_in_ms: f64,
  pub bus: Option<String>,
}

impl Default for PlayOptions {
  fn default() -> Self {
    PlayOptions { looping: false, gain: 1.0, pan: None, rate: None, fade_in_ms: 0.0, bus: None }
  }
}

/// The mixer's counters: what `/stats` shows about audio.
pub struct AudioStats {
  pub live_voices: usize,
  pub underruns: u64,
  /// The longest callback render pass since the last read, in ms.
  pub render_max_ms: f64,
  pub device_rate: u32,
  pub device_frames: u32,
  /// Frames rendered and held by the device stream, not yet consumed: the
  /// mix's lead over the device (positions are reported net of it).
  pub device_queued_frames: u64,
}

/// A loaded clip in the registry.
enum LoadedSound {
  /// Decoded whole; voices share it.
  Clip(Arc<Pcm>),
  /// Decoded on the decode thread into a ring one voice at a time reads.
  Stream(StreamClip),
}

struct StreamClip {
  control: Arc<StreamControl>,
  ring: Arc<RingShared>,
  format: ClipFormat,
  /// A play has happened: the next one restarts the decoder at 0 (the
  /// first finds the ring prefetched from 0 already).
  played: Cell<bool>,
}

/// A started voice as the registry knows it.
struct TrackHandle {
  slot: Slot,
  shared: Arc<VoiceShared>,
  sample_rate: u32,
  bus: Option<usize>,
  /// The streaming clip it reads, for loop changes and a restart.
  stream: Option<Arc<StreamControl>>,
  /// Which clip it plays (a stream's next play stops the voice before it).
  sound: Option<u64>,
}

struct SinkEntry {
  slot: Slot,
  shared: Arc<VoiceShared>,
  producer: Arc<Mutex<RingProducer>>,
  ring: Arc<RingShared>,
  sample_rate: u32,
  channels: u16,
}

/// A PCM sink driven from another thread (a video player's worker, a
/// runtime module): push, read and pause through the same ring and
/// command queue the registry uses. Holds the ring alive; a sink destroyed
/// while a handle is out leaves the handle a closed sink that refuses
/// pushes.
pub struct PcmSinkHandle {
  device: Arc<device::Shared>,
  producer: Arc<Mutex<RingProducer>>,
  ring: Arc<RingShared>,
  voice: Arc<VoiceShared>,
  slot: Slot,
  sample_rate: u32,
  channels: u16,
}

impl PcmSinkHandle {
  fn frames_to_us(&self, frames: u64) -> i64 {
    (frames as i128 * 1_000_000 / self.sample_rate as i128) as i64
  }

  /// Queue interleaved f32 samples (a whole number of frames). Errs when
  /// the sink is closed or holds too much already (push while `queued_us`
  /// is under your lookahead).
  pub fn push(&mut self, samples: &[f32]) -> Result<(), String> {
    if samples.len() % self.channels as usize != 0 {
      return Err(format!("{} samples is not whole {}-channel frames", samples.len(), self.channels));
    }
    if self.voice.ended() {
      return Err("pcm push failed: sink closed".into());
    }
    let frames = samples.len() / self.channels as usize;
    let mut producer = self.producer.lock().expect("sink producer lock poisoned");
    if producer.free() < frames {
      return Err(format!("pcm push failed: sink full ({} us queued)", self.frames_to_us(self.ring.queued())));
    }
    producer.push(samples);
    Ok(())
  }

  /// Microseconds pushed and not yet heard: what the ring holds plus what
  /// the voice's converter took ahead of the mix (as of the last block).
  pub fn queued_us(&self) -> i64 {
    self.frames_to_us(self.ring.queued() + self.voice.backlog.load(std::sync::atomic::Ordering::Acquire))
  }

  /// Microseconds consumed since creation: frames the mix has rendered off
  /// the queue (a clear counts what it dropped as consumed). Advances per
  /// mix block; `position_at_us` interpolates between blocks.
  pub fn position_us(&self) -> i64 {
    self.frames_to_us(sink_position_frames(&self.ring, &self.voice))
  }

  /// The position interpolated to `now`: the last block's position plus
  /// the frames the device has taken since, capped at that block's length.
  pub fn position_at_us(&self, now: Instant) -> i64 {
    let (position, at_nanos, block_frames) = self.voice.block();
    let elapsed = self.device.nanos_of(now).saturating_sub(at_nanos);
    let advanced = ((elapsed as u128 * self.sample_rate as u128) / 1_000_000_000) as u64;
    self.frames_to_us(position + advanced.min(block_frames))
  }

  /// Pause or resume consumption; paused, the queue holds.
  pub fn set_paused(&mut self, paused: bool) {
    self.device.send(Command::SetPaused { slot: self.slot, paused });
  }

  /// Drop everything queued (and start pushing again, after an `end`).
  pub fn clear(&mut self) {
    self.producer.lock().expect("sink producer lock poisoned").restart(0);
  }

  /// Nothing more is coming until a `clear`: the sink plays out what it
  /// holds, then renders silence without counting it as starvation.
  pub fn end(&mut self) {
    self.producer.lock().expect("sink producer lock poisoned").set_ended();
  }
}

/// A track's state for a handle that outlives the registry's entry (the
/// registry forgets a track once it ended; a playback object asks on):
/// lock-free reads of what the callback publishes per block.
#[derive(Clone)]
pub struct AudioTrackReader {
  shared: Arc<VoiceShared>,
  sample_rate: u32,
}

impl AudioTrackReader {
  pub fn ended(&self) -> bool {
    self.shared.ended()
  }

  pub fn paused(&self) -> bool {
    self.shared.paused()
  }

  /// The position in seconds of the clip, as of the last block; after the
  /// track ended, where it stopped.
  pub fn position(&self) -> f64 {
    self.shared.position() as f64 / self.sample_rate as f64
  }
}

/// A sink's consumed frames: the ring's consumed index (a clear counts at
/// once) less the frames the converter still holds as of the last block.
fn sink_position_frames(ring: &RingShared, voice: &VoiceShared) -> u64 {
  ring.consumed().saturating_sub(voice.backlog.load(std::sync::atomic::Ordering::Acquire))
}

#[derive(Default)]
pub struct AudioRegistry {
  // Opened on first use, kept for the process lifetime.
  device: RefCell<Option<Device>>,
  decoder: RefCell<Option<Decoder>>,
  tracks: RefCell<HashMap<u64, TrackHandle>>,
  next_id: Cell<u64>,
  sounds: RefCell<HashMap<u64, LoadedSound>>,
  next_sound_id: Cell<u64>,
  sinks: RefCell<HashMap<u64, SinkEntry>>,
  next_sink_id: Cell<u64>,
  buses: RefCell<HashMap<String, usize>>,
  // Voice table slots not in use, and each slot's generation so far.
  free_slots: RefCell<Option<Vec<u32>>>,
  generations: RefCell<Vec<u32>>,
}

impl AudioRegistry {
  /// The shared device state, opening the device on first use.
  fn device(&self) -> Result<Arc<device::Shared>, String> {
    let mut device = self.device.borrow_mut();
    if device.is_none() {
      *device = Some(Device::open()?);
    }
    Ok(Arc::clone(&device.as_ref().expect("device just opened").shared))
  }

  fn with_device<T>(&self, f: impl FnOnce(&Device) -> Result<T, String>) -> Result<T, String> {
    self.device()?;
    let device = self.device.borrow();
    f(device.as_ref().expect("device open"))
  }

  fn decoder(&self) -> std::cell::RefMut<'_, Decoder> {
    let mut decoder = self.decoder.borrow_mut();
    if decoder.is_none() {
      *decoder = Some(Decoder::spawn());
    }
    std::cell::RefMut::map(decoder, |d| d.as_mut().expect("decoder just spawned"))
  }

  fn next(&self, counter: &Cell<u64>) -> u64 {
    let id = counter.get() + 1;
    counter.set(id);
    id
  }

  /// A free slot with a fresh generation, or the cap error. The cap counts
  /// voices live on the control side (tracks and sinks not ended); a
  /// stopped voice frees capacity at once, its slot a block later, which
  /// the table's headroom absorbs.
  fn alloc_slot(&self) -> Result<Slot, String> {
    let live = self.tracks.borrow().values().filter(|t| !t.shared.ended()).count()
      + self.sinks.borrow().values().filter(|s| !s.shared.ended()).count();
    if live >= MAX_LIVE_TRACKS {
      return Err(format!("too many live playbacks (cap {MAX_LIVE_TRACKS}); stop some before starting more"));
    }
    let mut free = self.free_slots.borrow_mut();
    let free = free.get_or_insert_with(|| (0..SLOT_TABLE as u32).rev().collect());
    let index = free.pop().ok_or_else(|| "too many playbacks stopping at once; try again".to_string())?;
    let mut generations = self.generations.borrow_mut();
    if generations.is_empty() {
      generations.resize(SLOT_TABLE, 0);
    }
    generations[index as usize] += 1;
    Ok(Slot { index, generation: generations[index as usize] })
  }

  /// A stop that takes effect now (no fade): the track reads as ended at
  /// once, as the callback will make it within a block.
  fn mark_ended(track: &TrackHandle) {
    track.shared.ended.store(true, std::sync::atomic::Ordering::Release);
  }

  /// Free the voices the callback has retired (their converters go here,
  /// on this thread) and forget the tracks that ended.
  fn sweep_finished(&self) {
    let Some(shared) = self.device.borrow().as_ref().map(|d| Arc::clone(&d.shared)) else { return };
    let retired = shared.take_retired();
    if !retired.is_empty() {
      let mut free = self.free_slots.borrow_mut();
      let free = free.get_or_insert_with(Vec::new);
      for voice in retired {
        free.push(voice.slot.index);
      }
    }
    self.tracks.borrow_mut().retain(|_, track| !track.shared.ended());
  }

  /// The bus index for a name, created by use.
  fn bus_index(&self, name: &str) -> Result<usize, String> {
    let mut buses = self.buses.borrow_mut();
    if let Some(&index) = buses.get(name) {
      return Ok(index);
    }
    if buses.len() >= MAX_BUSES {
      return Err(format!("too many buses (cap {MAX_BUSES})"));
    }
    let index = buses.len();
    buses.insert(name.to_string(), index);
    Ok(index)
  }

  fn frames(&self, device: &device::Shared, ms: f64) -> u32 {
    (ms.max(0.0) / 1000.0 * device.device_rate as f64).round() as u32
  }

  /// Start a voice over `source` with `options`, returning its track id.
  fn start_voice(
    &self,
    source: Source,
    sample_rate: u32,
    channels: u16,
    options: &PlayOptions,
    stream: Option<Arc<StreamControl>>,
    sound: Option<u64>,
  ) -> Result<u64, String> {
    self.sweep_finished();
    let bus = match &options.bus {
      Some(name) => Some(self.bus_index(name)?),
      None => None,
    };
    let slot = self.alloc_slot()?;
    let shared = VoiceShared::new();
    let voice = self.with_device(|device| {
      let converter = device.converter(sample_rate, channels)?;
      let mut voice =
        Voice::new(slot, source, converter, channels, sample_rate, device.shared.device_rate, Arc::clone(&shared));
      voice.gain = Ramp::at(options.gain.max(0.0));
      voice.pan = options.pan.map(Ramp::at);
      voice.rate = Ramp::at(clamp_rate(options.rate.unwrap_or(1.0)));
      voice.looping = options.looping;
      voice.bus = bus;
      if options.fade_in_ms > 0.0 {
        voice.envelope = Ramp::at(0.0);
        voice.envelope.set(1.0, self.frames(&device.shared, options.fade_in_ms));
      }
      device.shared.send(Command::Start(Box::new(voice)));
      Ok(())
    });
    if let Err(e) = voice {
      self.free_slots.borrow_mut().get_or_insert_with(Vec::new).push(slot.index);
      return Err(e);
    }
    let id = self.next(&self.next_id);
    self.tracks.borrow_mut().insert(id, TrackHandle { slot, shared, sample_rate, bus, stream, sound });
    Ok(id)
  }

  fn send_to_track(&self, id: u64, make: impl FnOnce(Slot) -> Command) {
    let slot = self.tracks.borrow().get(&id).filter(|t| !t.shared.ended()).map(|t| t.slot);
    if let (Some(slot), Some(device)) = (slot, self.device.borrow().as_ref()) {
      device.shared.send(make(slot));
    }
  }
}

fn check_pcm(sample_rate: u32, channels: u16) -> Result<(), String> {
  if sample_rate == 0 {
    return Err("sample rate is 0".into());
  }
  if channels != 1 && channels != 2 {
    return Err(format!("{channels} channels is not supported; only mono and stereo play"));
  }
  Ok(())
}

impl crate::context::Context {
  /// Start a decoded clip with the given `PlayOptions`, fire-and-forget:
  /// the voice alone holds the clip. Returns the track id, usable with
  /// `stop_audio`.
  pub fn play_audio(&self, pcm: Pcm, options: &PlayOptions) -> Result<u64, String> {
    check_pcm(pcm.sample_rate, pcm.channels)?;
    let (sample_rate, channels) = (pcm.sample_rate, pcm.channels);
    let source = Source::Clip { pcm: Arc::new(pcm), cursor: 0 };
    self.audio.start_voice(source, sample_rate, channels, options, None, None)
  }

  /// Retain a decoded clip, returning a sound id. Replay it cheaply with
  /// `play_sound` (overlapping voices share the samples); release it with
  /// `unload_sound`. Non-finite samples are rejected: they are synthesis
  /// bugs, not audio. Opens the device, so a box without one fails here.
  pub fn load_sound(&self, pcm: Pcm) -> Result<u64, String> {
    check_pcm(pcm.sample_rate, pcm.channels)?;
    if !pcm_all_finite(&pcm.samples) {
      return Err("samples must be finite (found NaN or infinity)".into());
    }
    self.audio.device()?;
    let id = self.audio.next(&self.audio.next_sound_id);
    self.audio.sounds.borrow_mut().insert(id, LoadedSound::Clip(Arc::new(pcm)));
    Ok(id)
  }

  /// Open a clip for streaming: decoded on the decode thread ahead of
  /// playback, so a large track needs little RAM. Returns a sound id used
  /// like `load_sound`'s. A streaming clip plays one voice at a time: a
  /// `play_sound` while it plays restarts it from the beginning.
  pub fn stream_sound(&self, source: Box<dyn ClipSource>) -> Result<u64, String> {
    let format = source.format();
    check_pcm(format.sample_rate, format.channels)?;
    self.audio.device()?;
    let capacity = (format.sample_rate as f64 * stream::STREAM_RING_SECONDS) as usize;
    let (producer, consumer) = ring::ring(capacity, format.channels as usize);
    let ring = Arc::clone(consumer.shared());
    drop(consumer);
    let control = self.audio.decoder().add(source, producer, format.channels);
    let id = self.audio.next(&self.audio.next_sound_id);
    self
      .audio
      .sounds
      .borrow_mut()
      .insert(id, LoadedSound::Stream(StreamClip { control, ring, format, played: Cell::new(false) }));
    Ok(id)
  }

  /// Start a fresh voice for a loaded sound, returning a track id usable
  /// with `stop_audio`. Each call on a decoded clip is a new overlapping
  /// voice; on a streaming clip it restarts the single voice. Ids never
  /// leave the runtime, so an unknown id means the clip was unloaded.
  pub fn play_sound(&self, sound_id: u64, options: &PlayOptions) -> Result<u64, String> {
    let sounds = self.audio.sounds.borrow();
    let sound = sounds.get(&sound_id).ok_or_else(|| "clip has been unloaded".to_string())?;
    match sound {
      LoadedSound::Clip(pcm) => {
        let source = Source::Clip { pcm: Arc::clone(pcm), cursor: 0 };
        self.audio.start_voice(source, pcm.sample_rate, pcm.channels, options, None, Some(sound_id))
      }
      LoadedSound::Stream(clip) => {
        // The voice playing it, if any, stops first (in command order,
        // before the new one starts).
        let playing: Vec<u64> =
          self.audio.tracks.borrow().iter().filter(|(_, t)| t.sound == Some(sound_id)).map(|(id, _)| *id).collect();
        for id in playing {
          self.stop_audio(id, 0.0);
        }
        clip.control.looping.store(options.looping, std::sync::atomic::Ordering::Release);
        let generation = if clip.played.replace(true) {
          clip.control.restart_at(0);
          clip.ring.generation() + 1
        } else {
          clip.ring.generation()
        };
        self.audio.decoder().wake();
        let source =
          Source::Stream { ring: RingConsumer::over(Arc::clone(&clip.ring)), frames: clip.format.frames, generation };
        self.audio.start_voice(
          source,
          clip.format.sample_rate,
          clip.format.channels,
          options,
          Some(Arc::clone(&clip.control)),
          Some(sound_id),
        )
      }
    }
  }

  /// Change the gain of a playing track, ramping linearly over `ramp_ms`
  /// when it is > 0 (0 = at once, cancelling any gain ramp in flight). A
  /// voice that already finished is silently skipped: live control races
  /// with natural completion by design, so a late set is not an error.
  pub fn set_audio_gain(&self, id: u64, gain: f32, ramp_ms: f64) -> Result<(), String> {
    let device = self.audio.device()?;
    self.audio.send_to_track(id, |slot| Command::SetGain {
      slot,
      gain: gain.max(0.0),
      frames: self.audio.frames(&device, ramp_ms),
    });
    Ok(())
  }

  /// Position a playing track in the stereo field, pan in [-1, 1]
  /// (clamped), 0 = center; equal-power law. Ramps over `ramp_ms` like
  /// `set_audio_gain` (a never-panned voice ramps from center).
  pub fn set_audio_pan(&self, id: u64, pan: f32, ramp_ms: f64) -> Result<(), String> {
    let device = self.audio.device()?;
    self.audio.send_to_track(id, |slot| Command::SetPan { slot, pan, frames: self.audio.frames(&device, ramp_ms) });
    Ok(())
  }

  /// Change the playback rate of a playing track, 1.0 = as loaded, higher
  /// is faster and higher-pitched (clamped to the resampler's range).
  /// Ramps over `ramp_ms`, stepped per mix block.
  pub fn set_audio_rate(&self, id: u64, rate: f32, ramp_ms: f64) -> Result<(), String> {
    let device = self.audio.device()?;
    self.audio.send_to_track(id, |slot| Command::SetRate {
      slot,
      rate: clamp_rate(rate),
      frames: self.audio.frames(&device, ramp_ms),
    });
    Ok(())
  }

  /// Whether a playing track repeats: turned off, it finishes its current
  /// pass and ends.
  pub fn set_audio_loop(&self, id: u64, looping: bool) {
    if let Some(track) = self.audio.tracks.borrow().get(&id) {
      if let Some(stream) = &track.stream {
        stream.looping.store(looping, std::sync::atomic::Ordering::Release);
        self.audio.decoder().wake();
      }
    }
    self.audio.send_to_track(id, |slot| Command::SetLoop { slot, looping });
  }

  /// Pause or resume a track: paused, it renders nothing and its position
  /// stands.
  pub fn set_audio_paused(&self, id: u64, paused: bool) {
    self.audio.send_to_track(id, |slot| Command::SetPaused { slot, paused });
  }

  /// A reader on a track's state (position, paused, ended) that stays valid
  /// after the registry forgets the track; None for an unknown id.
  pub fn audio_track_reader(&self, id: u64) -> Option<AudioTrackReader> {
    let tracks = self.audio.tracks.borrow();
    let track = tracks.get(&id)?;
    Some(AudioTrackReader { shared: Arc::clone(&track.shared), sample_rate: track.sample_rate })
  }

  /// Pause or resume every track (sinks excepted: their owners pause them).
  pub fn pause_all_audio(&self, paused: bool) {
    let Some(device) = self.audio.device.borrow().as_ref().map(|d| Arc::clone(&d.shared)) else { return };
    for track in self.audio.tracks.borrow().values().filter(|t| !t.shared.ended()) {
      device.send(Command::SetPaused { slot: track.slot, paused });
    }
  }

  /// A loaded sound's length in seconds, when known; None for an unknown
  /// id or a stream whose container does not say.
  pub fn sound_duration(&self, sound_id: u64) -> Option<f64> {
    match self.audio.sounds.borrow().get(&sound_id)? {
      LoadedSound::Clip(pcm) => Some(pcm.frames() as f64 / pcm.sample_rate as f64),
      LoadedSound::Stream(clip) => clip.format.frames.map(|f| f as f64 / clip.format.sample_rate as f64),
    }
  }

  /// Scale the whole mix: every voice, sinks included (1.0 = unchanged, 0
  /// = silence), ramping over `ramp_ms` like `set_audio_gain`. Opens the
  /// device if needed, so a gain set before the first play still applies.
  pub fn set_master_gain(&self, gain: f32, ramp_ms: f64) -> Result<(), String> {
    let device = self.audio.device()?;
    device.send(Command::SetMasterGain { gain: gain.max(0.0), frames: self.audio.frames(&device, ramp_ms) });
    Ok(())
  }

  /// Scale one bus: every voice on it, live and future, under its own gain
  /// and over the master (1.0 = unchanged). Ramps like `set_audio_gain`.
  pub fn set_bus_gain(&self, bus: &str, gain: f32, ramp_ms: f64) -> Result<(), String> {
    let device = self.audio.device()?;
    let bus = self.audio.bus_index(bus)?;
    device.send(Command::SetBusGain { bus, gain: gain.max(0.0), frames: self.audio.frames(&device, ramp_ms) });
    Ok(())
  }

  /// Pause or resume one bus: its voices, live and future, render nothing
  /// while it is paused.
  pub fn set_bus_paused(&self, bus: &str, paused: bool) -> Result<(), String> {
    let device = self.audio.device()?;
    let bus = self.audio.bus_index(bus)?;
    device.send(Command::SetBusPaused { bus, paused });
    Ok(())
  }

  /// The device's sample rate in Hz. Synthesizing PCM at this rate avoids a
  /// resample on every voice. Opens the device if needed.
  pub fn audio_output_sample_rate(&self) -> Result<i32, String> {
    Ok(self.audio.device()?.device_rate as i32)
  }

  /// Whether a track finished playing, naturally or via stop. An unknown
  /// id reads as ended: finished tracks are swept from the registry, so
  /// absence means the voice is gone.
  pub fn audio_ended(&self, id: u64) -> bool {
    self.audio.tracks.borrow().get(&id).map_or(true, |t| t.shared.ended())
  }

  /// Release a loaded sound. Voices playing a decoded clip keep going (they
  /// share the samples) until they stop on their own; a streaming clip's
  /// voice plays out what its ring holds and ends.
  pub fn unload_sound(&self, sound_id: u64) {
    if let Some(LoadedSound::Stream(clip)) = self.audio.sounds.borrow_mut().remove(&sound_id) {
      clip.control.close();
      self.audio.decoder().wake();
    }
  }

  /// Stop a single track, fading to silence over `fade_out_ms` first when
  /// it is > 0 (the track keeps playing while it fades and reads as ended
  /// once the fade completes). A no-op if it already finished.
  pub fn stop_audio(&self, id: u64, fade_out_ms: f64) {
    let Some(device) = self.audio.device.borrow().as_ref().map(|d| Arc::clone(&d.shared)) else { return };
    let fade_frames = self.audio.frames(&device, fade_out_ms);
    self.audio.send_to_track(id, |slot| Command::Stop { slot, fade_frames });
    if fade_frames == 0 {
      if let Some(track) = self.audio.tracks.borrow().get(&id) {
        AudioRegistry::mark_ended(track);
      }
    }
  }

  /// Stop every voice, sinks excepted (they are closed by their owners),
  /// fading the whole mix out over `fade_out_ms` first when it is > 0.
  pub fn stop_all_audio(&self, fade_out_ms: f64) {
    let Some(device) = self.audio.device.borrow().as_ref().map(|d| Arc::clone(&d.shared)) else { return };
    let fade_frames = self.audio.frames(&device, fade_out_ms);
    for track in self.audio.tracks.borrow().values().filter(|t| !t.shared.ended()) {
      device.send(Command::Stop { slot: track.slot, fade_frames });
      if fade_frames == 0 {
        AudioRegistry::mark_ended(track);
      }
    }
  }

  /// Stop every voice on one bus (see `PlayOptions::bus`), fading it out
  /// over `fade_out_ms` first when it is > 0.
  pub fn stop_bus_audio(&self, bus: &str, fade_out_ms: f64) {
    let Some(device) = self.audio.device.borrow().as_ref().map(|d| Arc::clone(&d.shared)) else { return };
    let Some(&bus) = self.audio.buses.borrow().get(bus) else { return };
    let fade_frames = self.audio.frames(&device, fade_out_ms);
    device.send(Command::StopBus { bus, fade_frames });
    if fade_frames == 0 {
      for track in self.audio.tracks.borrow().values().filter(|t| t.bus == Some(bus)) {
        AudioRegistry::mark_ended(track);
      }
    }
  }

  /// The mixer's counters, for stats.
  pub fn audio_stats(&self) -> Option<AudioStats> {
    let device = self.audio.device.borrow();
    let shared = &device.as_ref()?.shared;
    let (engine, render_max_nanos, device_queued_frames) = shared.stats();
    Some(AudioStats {
      live_voices: engine.live_voices,
      underruns: engine.underruns,
      render_max_ms: render_max_nanos as f64 / 1_000_000.0,
      device_rate: shared.device_rate,
      device_frames: shared.device_frames,
      device_queued_frames,
    })
  }

  /// Open a streaming PCM sink: interleaved f32 at `sample_rate`/
  /// `channels`, fed with `pcm_sink_push` (or a handle) and mixed like any
  /// voice, under the master and, when `bus` names one, that bus. Starts
  /// playing (an empty sink is silence); `set_pcm_sink_paused` gates it.
  /// Returns the sink id.
  pub fn create_pcm_sink(&self, sample_rate: u32, channels: u16, bus: Option<&str>) -> Result<u64, String> {
    check_pcm(sample_rate, channels).map_err(|e| format!("invalid pcm sink spec: {e}"))?;
    self.audio.sweep_finished();
    let device = self.audio.device()?;
    let bus_index = match bus {
      Some(name) => Some(self.audio.bus_index(name)?),
      None => None,
    };
    let capacity = (sample_rate as f64 * SINK_RING_SECONDS) as usize;
    let (producer, consumer) = ring::ring(capacity, channels as usize);
    let ring = Arc::clone(consumer.shared());
    let slot = self.audio.alloc_slot()?;
    let shared = VoiceShared::new();
    let started = self.audio.with_device(|dev| {
      let converter = dev.converter(sample_rate, channels)?;
      let mut voice = Voice::new(
        slot,
        Source::Sink { ring: consumer },
        converter,
        channels,
        sample_rate,
        device.device_rate,
        Arc::clone(&shared),
      );
      voice.bus = bus_index;
      device.send(Command::Start(Box::new(voice)));
      Ok(())
    });
    if let Err(e) = started {
      self.audio.free_slots.borrow_mut().get_or_insert_with(Vec::new).push(slot.index);
      return Err(e);
    }
    let id = self.audio.next(&self.audio.next_sink_id);
    self
      .audio
      .sinks
      .borrow_mut()
      .insert(id, SinkEntry { slot, shared, producer: Arc::new(Mutex::new(producer)), ring, sample_rate, channels });
    Ok(id)
  }

  /// A handle on a sink for another thread to drive (see `PcmSinkHandle`).
  pub fn pcm_sink_handle(&self, id: u64) -> Result<PcmSinkHandle, String> {
    let sinks = self.audio.sinks.borrow();
    let sink = sinks.get(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    Ok(PcmSinkHandle {
      device: self.audio.device()?,
      producer: Arc::clone(&sink.producer),
      ring: Arc::clone(&sink.ring),
      voice: Arc::clone(&sink.shared),
      slot: sink.slot,
      sample_rate: sink.sample_rate,
      channels: sink.channels,
    })
  }

  /// Queue interleaved f32 samples on a sink (non-blocking). The sample
  /// count must be a whole number of frames.
  pub fn pcm_sink_push(&self, id: u64, samples: &[f32]) -> Result<(), String> {
    self.pcm_sink_handle(id)?.push(samples)
  }

  /// The sink's playback position in microseconds: frames the mix has
  /// consumed off its queue. This is the master clock for A/V sync.
  pub fn pcm_sink_position_us(&self, id: u64) -> Result<i64, String> {
    Ok(self.pcm_sink_handle(id)?.position_us())
  }

  /// Microseconds of audio queued and not yet consumed: the pusher's
  /// backpressure signal (stop pushing above a lookahead threshold).
  pub fn pcm_sink_queued_us(&self, id: u64) -> Result<i64, String> {
    Ok(self.pcm_sink_handle(id)?.queued_us())
  }

  /// Pause or resume consumption. Paused, the queue holds and the position
  /// freezes, which is exactly what pauses video against the audio clock.
  pub fn set_pcm_sink_paused(&self, id: u64, paused: bool) -> Result<(), String> {
    self.pcm_sink_handle(id)?.set_paused(paused);
    Ok(())
  }

  /// Scale the sink's volume (1.0 = unchanged), ramping over `ramp_ms`.
  pub fn set_pcm_sink_gain(&self, id: u64, gain: f32, ramp_ms: f64) -> Result<(), String> {
    let device = self.audio.device()?;
    let sinks = self.audio.sinks.borrow();
    let sink = sinks.get(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    device.send(Command::SetGain { slot: sink.slot, gain: gain.max(0.0), frames: self.audio.frames(&device, ramp_ms) });
    Ok(())
  }

  /// Drop everything a sink has queued.
  pub fn clear_pcm_sink(&self, id: u64) -> Result<(), String> {
    self.pcm_sink_handle(id)?.clear();
    Ok(())
  }

  /// Close a sink: queued audio is dropped, not drained (closing is a stop,
  /// not a fade-out); handles out on it refuse further pushes.
  pub fn destroy_pcm_sink(&self, id: u64) {
    if let Some(sink) = self.audio.sinks.borrow_mut().remove(&id) {
      if let Some(device) = self.audio.device.borrow().as_ref() {
        device.shared.send(Command::Stop { slot: sink.slot, fade_frames: 0 });
      }
      sink.shared.ended.store(true, std::sync::atomic::Ordering::Release);
    }
  }
}
