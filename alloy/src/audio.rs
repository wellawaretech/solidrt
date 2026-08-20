//! Sound playback via SDL_mixer.
//!
//! `play_audio` decodes an encoded clip (Ogg/Vorbis or WAV) and starts it on a
//! fresh mixer track, returning an id the caller can later `stop`. Tracks are
//! retained in the registry so a sound keeps playing after the call returns and
//! can be stopped individually; finished tracks are swept on the next play so a
//! stream of fire-and-forget sounds does not accumulate.
//!
//! There is no per-frame pump: the SDL_mixer device runs its own audio thread.
//! Everything here is touched only from the UI/JS thread, like the microphone.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use sdl3::iostream::IOStream;
use sdl3::mixer::{Audio, Mixer, StereoGains, Track};
use sdl3::properties::{Properties, Setter};
use sdl3::sys::audio::{SDL_AudioFormat, SDL_AudioSpec, SDL_AUDIO_F32, SDL_AUDIO_S16, SDL_AUDIO_U8};

use crate::sdl_utils;

/// Sample format of a raw PCM clip, as handed to `load_pcm_sound`. Multi-byte
/// formats are native-endian, matching what a JS TypedArray holds in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcmFormat {
  U8,
  S16,
  F32,
}

impl PcmFormat {
  fn to_sdl(self) -> SDL_AudioFormat {
    match self {
      PcmFormat::U8 => SDL_AUDIO_U8,
      PcmFormat::S16 => SDL_AUDIO_S16,
      PcmFormat::F32 => SDL_AUDIO_F32,
    }
  }
}

/// Map a pan position in [-1, 1] (clamped) to per-channel gains using the
/// equal-power law, so perceived loudness stays constant as a sound sweeps
/// across the field. Center is (0.707, 0.707), the same 3 dB dip a Web Audio
/// StereoPannerNode applies to a mono source at pan 0.
pub(crate) fn pan_gains(pan: f32) -> StereoGains {
  // clamp propagates NaN; read NaN as center so the gains are always valid.
  let pan = if pan.is_nan() { 0.0 } else { pan.clamp(-1.0, 1.0) };
  let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
  StereoGains { left: angle.cos(), right: angle.sin() }
}

/// Clamp a playback rate to the range SDL's resampler supports
/// (MIX_SetTrackFrequencyRatio: 0.01..100). NaN reads as 1.0 (unchanged),
/// like pan_gains reads NaN as center.
pub(crate) fn clamp_rate(rate: f32) -> f32 {
  if rate.is_nan() {
    1.0
  } else {
    rate.clamp(0.01, 100.0)
  }
}

/// Whether every f32 sample in a raw byte buffer is finite. Non-finite
/// samples are the signature of a synthesis bug (a NaN poisons the mix), so
/// `load_pcm_sound` rejects them instead of playing a pop.
pub(crate) fn pcm_f32_all_finite(bytes: &[u8]) -> bool {
  bytes.chunks_exact(4).all(|c| f32::from_ne_bytes([c[0], c[1], c[2], c[3]]).is_finite())
}

/// Live-voice ceiling: `spawn_track` refuses to start a voice beyond this
/// many concurrent tracks. Far above any real mix (each track is a full
/// SDL_AudioStream); the cap turns a runaway play() loop into an error
/// instead of a wedged process.
const MAX_LIVE_TRACKS: usize = 256;

/// How often the ramp driver steps active ramps. 100 Hz control rate: fine
/// enough that parameter steps are inaudible on normal fades, and immune to
/// app frame hitches. Sample-accurate fades exist only at the edges (the
/// fade-in play option and the fade-out stop), where SDL interpolates per
/// sample frame.
const RAMP_STEP: Duration = Duration::from_millis(10);

/// Which live parameter a ramp steers.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum RampParam {
  Gain,
  Pan,
  Rate,
}

/// Ramps are keyed by track id + parameter; the master gain rides the
/// reserved id 0 (track ids start at 1).
type RampKey = (u64, RampParam);
const MASTER: u64 = 0;

/// Interpolate a ramp: linear in parameter space from `from` to `to` over
/// `duration`. Returns the value and whether the ramp is finished (a zero
/// duration is instantly done).
pub(crate) fn ramp_value(from: f32, to: f32, elapsed: Duration, duration: Duration) -> (f32, bool) {
  if duration.is_zero() || elapsed >= duration {
    return (to, true);
  }
  let t = (elapsed.as_secs_f64() / duration.as_secs_f64()) as f32;
  (from + (to - from) * t, false)
}

/// One in-flight parameter ramp. `ptr` is the raw MIX_Track (or MIX_Mixer for
/// the master) as usize so the table is Send; the purge protocol on
/// `RampState` keeps it valid for as long as the entry exists.
struct Ramp {
  ptr: usize,
  from: f32,
  to: f32,
  start: Instant,
  duration: Duration,
}

impl Ramp {
  fn value_at(&self, now: Instant) -> (f32, bool) {
    ramp_value(self.from, self.to, now.saturating_duration_since(self.start), self.duration)
  }
}

/// The ramp table plus the per-track pan shadow, shared between the JS thread
/// and the ramp thread under one lock.
///
/// Pointer-safety protocol: a `Ramp`'s `ptr` may only be passed to the
/// sdl_utils raw setters while holding this lock, and every code path that
/// drops a `Track` FIRST removes its entries here (stop, sweep, stop-all,
/// close-all). The SDL setters themselves are documented safe from any
/// thread; the lock is what keeps the pointer alive across the call.
#[derive(Default)]
struct RampState {
  ramps: HashMap<RampKey, Ramp>,
  /// Last effective pan per spatialized track. SDL has no pan getter, and a
  /// pan ramp must start from the current value.
  pans: HashMap<u64, f32>,
}

/// Owns the shared ramp state and the lazily-spawned `srt-audio-ramp` thread.
/// The thread steps every active ramp each `RAMP_STEP` and parks on the
/// condvar while the table is empty, so an app that never ramps never wakes.
#[derive(Default)]
struct RampDriver {
  state: Arc<(Mutex<RampState>, Condvar)>,
  thread_started: Cell<bool>,
}

impl RampDriver {
  fn lock(&self) -> std::sync::MutexGuard<'_, RampState> {
    self.state.0.lock().expect("audio ramp lock poisoned")
  }

  /// Begin ramping `(id, param)` to `to` over `duration`, replacing any ramp
  /// already running on that parameter from its current value. `fallback` is
  /// the current value when no ramp is active (the caller reads it from SDL;
  /// for pan the shadow takes precedence, since SDL cannot be asked).
  fn start(&self, id: u64, param: RampParam, ptr: usize, to: f32, duration: Duration, fallback: f32) {
    let (_, cvar) = &*self.state;
    {
      let mut state = self.lock();
      let now = Instant::now();
      let from = match state.ramps.get(&(id, param)) {
        Some(ramp) => ramp.value_at(now).0,
        None if param == RampParam::Pan => state.pans.get(&id).copied().unwrap_or(fallback),
        None => fallback,
      };
      state.ramps.insert((id, param), Ramp { ptr, from, to, start: now, duration });
    }
    cvar.notify_one();
    if !self.thread_started.get() {
      self.spawn_thread();
    }
  }

  /// Cancel one parameter's ramp: an immediate set supersedes it.
  fn cancel(&self, id: u64, param: RampParam) {
    self.lock().ramps.remove(&(id, param));
  }

  /// Record an immediate pan write (and cancel a pan ramp): the shadow is
  /// where the next pan ramp starts from.
  fn set_pan_shadow(&self, id: u64, pan: f32) {
    let mut state = self.lock();
    state.ramps.remove(&(id, RampParam::Pan));
    state.pans.insert(id, pan);
  }

  /// Drop all state for tracks about to be destroyed. Must run BEFORE the
  /// `Track`s are dropped; once this returns the ramp thread holds no pointer
  /// to them.
  fn purge_tracks(&self, ids: &[u64]) {
    if ids.is_empty() {
      return;
    }
    let mut state = self.lock();
    for id in ids {
      for param in [RampParam::Gain, RampParam::Pan, RampParam::Rate] {
        state.ramps.remove(&(*id, param));
      }
      state.pans.remove(id);
    }
  }

  /// Drop the state of every track (stop-all), keeping a master ramp alive.
  fn purge_all_tracks(&self) {
    let mut state = self.lock();
    state.ramps.retain(|key, _| key.0 == MASTER);
    state.pans.clear();
  }

  /// Drop everything, master included (between-runs close).
  fn purge_everything(&self) {
    let mut state = self.lock();
    state.ramps.clear();
    state.pans.clear();
  }

  fn spawn_thread(&self) {
    self.thread_started.set(true);
    let shared = Arc::clone(&self.state);
    let spawned = std::thread::Builder::new().name("srt-audio-ramp".into()).spawn(move || {
      let (lock, cvar) = &*shared;
      loop {
        let mut state = lock.lock().expect("audio ramp lock poisoned");
        while state.ramps.is_empty() {
          state = cvar.wait(state).expect("audio ramp wait poisoned");
        }
        let now = Instant::now();
        let RampState { ramps, pans } = &mut *state;
        ramps.retain(|&(id, param), ramp| {
          let (value, done) = ramp.value_at(now);
          // Setter errors are ignored: a set on a stopped track is harmless
          // and there is nothing actionable on this thread.
          match param {
            RampParam::Gain if id == MASTER => {
              crate::sdl_utils::mix_mixer_set_gain_raw(ramp.ptr, value.max(0.0));
            }
            RampParam::Gain => {
              crate::sdl_utils::mix_track_set_gain_raw(ramp.ptr, value.max(0.0));
            }
            RampParam::Pan => {
              let gains = pan_gains(value);
              crate::sdl_utils::mix_track_set_stereo_raw(ramp.ptr, gains.left, gains.right);
              pans.insert(id, value);
            }
            RampParam::Rate => {
              crate::sdl_utils::mix_track_set_frequency_ratio_raw(ramp.ptr, clamp_rate(value));
            }
          }
          !done
        });
        drop(state);
        std::thread::sleep(RAMP_STEP);
      }
    });
    spawned.expect("failed to spawn srt-audio-ramp thread");
  }
}

/// Options for starting a voice, shared by the fire-and-forget and clip
/// paths. `gain` defaults to 1.0 (as loaded); `pan`/`rate` of None leave the
/// voice unspatialized / at the loaded rate; `fade_in_ms` > 0 fades in from
/// silence, sample-accurately, via SDL's play option; `bus` names the group
/// the voice belongs to (an SDL tag), so `stop_bus_audio` can stop it as a
/// set. Buses are names only - no gain layer (see the mix-control backlog
/// item for why SDL tag gain is not one).
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

/// Convert a millisecond count from the API surface into a `Duration`.
fn ms_duration(ms: f64) -> Duration {
  Duration::from_secs_f64(ms.max(0.0) / 1000.0)
}

/// A loaded clip in the registry. A predecoded clip (`load_sound`) is just the
/// decoded `Audio`; a streamed clip (`stream_sound_io`) also retains its
/// `IOStream`, because SDL_mixer keeps decoding from it on demand (loaded with
/// `closeio=false`) and would read a freed source otherwise. Fields drop in
/// order, so `audio` (MIX_Audio) is destroyed before its backing `_io`.
struct LoadedSound {
  audio: Audio,
  _io: Option<IOStream<'static>>,
}

/// A streaming PCM sink: one plain SDL3 audio stream on the default playback
/// device, fed interleaved f32 by pushes. Built deliberately NOT on
/// SDL3_mixer - SDL3 mixes all bound streams natively, and this sink is the
/// pilot for replacing the mixer outright (okf/backlog/video-playback.md,
/// staging item 6). First consumer is video audio, whose consumed-samples
/// position doubles as the playback master clock.
struct PcmSink {
  stream: *mut sdl3::sys::audio::SDL_AudioStream,
  sample_rate: u32,
  channels: u16,
  /// Frames pushed since creation; position = pushed - queued.
  pushed_frames: u64,
}

impl Drop for PcmSink {
  fn drop(&mut self) {
    // Destroying the stream also closes the device it opened.
    sdl_utils::audio_stream_destroy(self.stream);
  }
}

#[derive(Default)]
pub struct AudioRegistry {
  // The mixer (and its MIX_Init guard) are opened once and leaked to `'static`
  // so tracks, which borrow the mixer, can be stored here. The device stays
  // open for the process lifetime; leaking is a one-time cost guarded by the
  // `Some` check in `mixer`.
  mixer: RefCell<Option<&'static Mixer>>,
  tracks: RefCell<HashMap<u64, Track<'static>>>,
  next_id: RefCell<u64>,
  // Live parameter ramps, stepped by the srt-audio-ramp thread (see RampDriver).
  ramps: RampDriver,
  // Decoded clips retained so a sound is decoded once and replayed cheaply.
  // Audio is independent of the mixer and ref-counted by the C library, so a
  // clip can be unloaded while its voices keep playing.
  sounds: RefCell<HashMap<u64, LoadedSound>>,
  next_sound_id: RefCell<u64>,
  // Streaming PCM sinks, their own id space (see PcmSink).
  sinks: RefCell<HashMap<u64, PcmSink>>,
  next_sink_id: RefCell<u64>,
}

impl AudioRegistry {
  /// Lazily open the playback device, returning the shared `'static` mixer.
  fn mixer(&self) -> Result<&'static Mixer, String> {
    if let Some(mixer) = *self.mixer.borrow() {
      return Ok(mixer);
    }
    if !sdl_utils::audio_subsystem_init() {
      return Err(format!("audio subsystem init failed: {}", sdl_utils::sdl_error()));
    }
    // Forget the MIX_Init guard so the library stays initialized for the process
    // lifetime; its Drop would call MIX_Quit and tear the mixer down.
    let ctx = sdl3::mixer::init().map_err(|e| format!("mixer init failed: {e}"))?;
    std::mem::forget(ctx);
    let mixer = Mixer::open_device(None).map_err(|e| format!("failed to open audio device: {e}"))?;
    let mixer: &'static Mixer = Box::leak(Box::new(mixer));
    *self.mixer.borrow_mut() = Some(mixer);
    Ok(mixer)
  }

  /// Drop tracks that have finished playing so the map does not grow with each
  /// fire-and-forget sound. A track that is playing or paused is kept. Ramp
  /// entries are purged FIRST: the ramp thread must never see a dropped track.
  fn sweep_finished(&self) {
    let finished: Vec<u64> = self
      .tracks
      .borrow()
      .iter()
      .filter(|(_, t)| !t.is_playing() && !t.is_paused())
      .map(|(id, _)| *id)
      .collect();
    if finished.is_empty() {
      return;
    }
    self.ramps.purge_tracks(&finished);
    let mut tracks = self.tracks.borrow_mut();
    for id in &finished {
      tracks.remove(id);
    }
  }

  /// Start a fresh voice for an already-decoded clip and retain it, returning
  /// the track id. Shared by the fire-and-forget path and `play_sound`.
  fn spawn_track(&self, mixer: &'static Mixer, audio: &Audio, options: &PlayOptions) -> Result<u64, String> {
    // Callers sweep finished tracks first, so this counts live voices.
    if self.tracks.borrow().len() >= MAX_LIVE_TRACKS {
      return Err(format!("too many live playbacks (cap {MAX_LIVE_TRACKS}); stop some before starting more"));
    }
    let track = mixer.create_track().map_err(|e| format!("failed to create audio track: {e}"))?;
    track.set_audio(audio).map_err(|e| format!("failed to assign audio: {e}"))?;
    track.set_gain(options.gain).map_err(|e| format!("failed to set gain: {e}"))?;
    if let Some(pan) = options.pan {
      track.set_stereo(Some(pan_gains(pan))).map_err(|e| format!("failed to set pan: {e}"))?;
    }
    if let Some(rate) = options.rate {
      track.set_frequency_ratio(clamp_rate(rate)).map_err(|e| format!("failed to set rate: {e}"))?;
    }
    if let Some(bus) = &options.bus {
      track.tag(bus).map_err(|e| format!("failed to tag bus: {e}"))?;
    }
    // MIX_PlayTrack resets the loop count from its play options every call, so a
    // prior set_loops is ignored (see MIX_PROP_PLAY_LOOPS_NUMBER). Pass the loop
    // count through the play options instead (-1 loops forever); the fade-in
    // rides the same property bag.
    if options.looping || options.fade_in_ms > 0.0 {
      let opts = Properties::new().map_err(|e| format!("failed to alloc play options: {e:?}"))?;
      if options.looping {
        opts.set("SDL_mixer.play.loops", -1i64).map_err(|e| format!("failed to set loop option: {e:?}"))?;
      }
      if options.fade_in_ms > 0.0 {
        opts
          .set("SDL_mixer.play.fade_in_milliseconds", options.fade_in_ms as i64)
          .map_err(|e| format!("failed to set fade-in option: {e:?}"))?;
      }
      track.play_with_options(&opts).map_err(|e| format!("failed to play audio: {e}"))?;
    } else {
      track.play().map_err(|e| format!("failed to play audio: {e}"))?;
    }
    let id = {
      let mut next = self.next_id.borrow_mut();
      *next += 1;
      *next
    };
    if let Some(pan) = options.pan {
      self.ramps.set_pan_shadow(id, pan);
    }
    self.tracks.borrow_mut().insert(id, track);
    Ok(id)
  }
}

impl crate::context::Context {
  /// Decode and start an encoded audio clip (Ogg/Vorbis or WAV) with the given
  /// `PlayOptions`. Returns the track id, usable with `stop_audio`.
  pub fn play_audio(&self, bytes: &[u8], options: &PlayOptions) -> Result<u64, String> {
    self.audio.sweep_finished();
    let mixer = self.audio.mixer()?;
    // predecode=true fully decodes into the Audio at load time, so neither the
    // IOStream nor `bytes` needs to outlive this call.
    let io = IOStream::from_bytes(bytes).map_err(|e| format!("audio read failed: {e}"))?;
    let audio = mixer.load_audio_io(&io, true).map_err(|e| format!("audio decode failed: {e}"))?;
    // The mixer ref-counts audio assigned to a track, so dropping `audio` at the
    // end of this call is safe: the track keeps the decoded data alive.
    self.audio.spawn_track(mixer, &audio, options)
  }

  /// Decode an encoded clip once and retain it, returning a sound id. Replay it
  /// cheaply with `play_sound` (no re-decode); release it with `unload_sound`.
  pub fn load_sound(&self, bytes: &[u8]) -> Result<u64, String> {
    let mixer = self.audio.mixer()?;
    let io = IOStream::from_bytes(bytes).map_err(|e| format!("audio read failed: {e}"))?;
    let audio = mixer.load_audio_io(&io, true).map_err(|e| format!("audio decode failed: {e}"))?;
    let id = {
      let mut next = self.audio.next_sound_id.borrow_mut();
      *next += 1;
      *next
    };
    self.audio.sounds.borrow_mut().insert(id, LoadedSound { audio, _io: None });
    Ok(id)
  }

  /// Open a seekable byte source for streaming playback: it is decoded on demand
  /// rather than fully into memory, so a large track needs little RAM. Returns a
  /// sound id used the same way as `load_sound`. A streaming clip carries decode
  /// state, so play it as a single voice (do not overlap it with itself), and
  /// stop its voice before `unload_sound` (SDL keeps decoding from the retained
  /// source until then; the reactive layer stops before unloading).
  pub fn stream_sound_io<R: std::io::Read + std::io::Seek + Send + 'static>(&self, reader: R) -> Result<u64, String> {
    let mixer = self.audio.mixer()?;
    let io = sdl_utils::iostream_from_reader(reader)?;
    // predecode=false: SDL decodes on demand and keeps referencing `io`, so it
    // is retained alongside the Audio (see LoadedSound).
    let audio = mixer.load_audio_io(&io, false).map_err(|e| format!("audio open failed: {e}"))?;
    let id = {
      let mut next = self.audio.next_sound_id.borrow_mut();
      *next += 1;
      *next
    };
    self.audio.sounds.borrow_mut().insert(id, LoadedSound { audio, _io: Some(io) });
    Ok(id)
  }

  /// Load raw PCM samples as a clip, returning a sound id used the same way as
  /// `load_sound`. No decoding happens: `spec` metadata is all SDL needs to play
  /// the bytes directly. The data is copied, so `bytes` need not outlive the call.
  /// Non-finite f32 samples are rejected: they are synthesis bugs, not audio.
  pub fn load_pcm_sound(&self, bytes: &[u8], sample_rate: i32, channels: i32, format: PcmFormat) -> Result<u64, String> {
    if format == PcmFormat::F32 && !pcm_f32_all_finite(bytes) {
      return Err("samples must be finite (found NaN or infinity)".into());
    }
    let mixer = self.audio.mixer()?;
    let spec = SDL_AudioSpec { format: format.to_sdl(), channels, freq: sample_rate };
    let audio = mixer.load_raw_audio(bytes, &spec).map_err(|e| format!("pcm load failed: {e}"))?;
    let id = {
      let mut next = self.audio.next_sound_id.borrow_mut();
      *next += 1;
      *next
    };
    self.audio.sounds.borrow_mut().insert(id, LoadedSound { audio, _io: None });
    Ok(id)
  }

  /// Start a fresh voice for a loaded sound, returning a track id usable with
  /// `stop_audio`. Each call is a new overlapping voice; no decode happens here.
  /// Ids never leave the runtime, so an unknown id means the clip was unloaded.
  pub fn play_sound(&self, sound_id: u64, options: &PlayOptions) -> Result<u64, String> {
    self.audio.sweep_finished();
    let mixer = self.audio.mixer()?;
    let sounds = self.audio.sounds.borrow();
    let sound = sounds.get(&sound_id).ok_or_else(|| "clip has been unloaded".to_string())?;
    self.audio.spawn_track(mixer, &sound.audio, options)
  }

  /// Change the gain of a playing track, ramping linearly over `ramp_ms` when
  /// it is > 0 (0 = immediately, cancelling any gain ramp in flight). A voice
  /// that already finished (or was stopped) is silently skipped: live control
  /// races with natural completion by design, so a late set is not an error.
  pub fn set_audio_gain(&self, id: u64, gain: f32, ramp_ms: f64) -> Result<(), String> {
    match self.audio.tracks.borrow().get(&id) {
      Some(track) if ramp_ms > 0.0 => {
        self.audio.ramps.start(id, RampParam::Gain, track.raw() as usize, gain.max(0.0), ms_duration(ramp_ms), track.gain());
        Ok(())
      }
      Some(track) => {
        self.audio.ramps.cancel(id, RampParam::Gain);
        track.set_gain(gain).map_err(|e| format!("failed to set gain: {e}"))
      }
      None => Ok(()),
    }
  }

  /// Position a playing track in the stereo field, pan in [-1, 1] (clamped),
  /// 0 = center; see `pan_gains` for the law. Ramps over `ramp_ms` like
  /// `set_audio_gain` (a never-panned voice ramps from center). Silently
  /// skipped for a finished voice.
  pub fn set_audio_pan(&self, id: u64, pan: f32, ramp_ms: f64) -> Result<(), String> {
    match self.audio.tracks.borrow().get(&id) {
      Some(track) if ramp_ms > 0.0 => {
        self.audio.ramps.start(id, RampParam::Pan, track.raw() as usize, pan, ms_duration(ramp_ms), 0.0);
        Ok(())
      }
      Some(track) => {
        self.audio.ramps.set_pan_shadow(id, pan);
        track.set_stereo(Some(pan_gains(pan))).map_err(|e| format!("failed to set pan: {e}"))
      }
      None => Ok(()),
    }
  }

  /// Change the playback rate of a playing track, 1.0 = as loaded, higher is
  /// faster and higher-pitched (see `clamp_rate` for the range). Ramps over
  /// `ramp_ms` like `set_audio_gain`. Silently skipped for a finished voice.
  pub fn set_audio_rate(&self, id: u64, rate: f32, ramp_ms: f64) -> Result<(), String> {
    match self.audio.tracks.borrow().get(&id) {
      Some(track) if ramp_ms > 0.0 => {
        self.audio.ramps.start(id, RampParam::Rate, track.raw() as usize, clamp_rate(rate), ms_duration(ramp_ms), track.frequency_ratio());
        Ok(())
      }
      Some(track) => {
        self.audio.ramps.cancel(id, RampParam::Rate);
        track.set_frequency_ratio(clamp_rate(rate)).map_err(|e| format!("failed to set rate: {e}"))
      }
      None => Ok(()),
    }
  }

  /// Scale the whole mix: every playing and future track (1.0 = unchanged,
  /// 0 = silence), ramping over `ramp_ms` like `set_audio_gain`. PCM sinks
  /// are separate SDL streams and unaffected. Opens the device if needed, so
  /// a gain set before the first play still applies.
  pub fn set_master_gain(&self, gain: f32, ramp_ms: f64) -> Result<(), String> {
    let mixer = self.audio.mixer()?;
    if ramp_ms > 0.0 {
      self.audio.ramps.start(MASTER, RampParam::Gain, mixer.raw() as usize, gain.max(0.0), ms_duration(ramp_ms), mixer.gain());
      Ok(())
    } else {
      self.audio.ramps.cancel(MASTER, RampParam::Gain);
      mixer.set_gain(gain.max(0.0)).map_err(|e| format!("failed to set master gain: {e}"))
    }
  }

  /// Whether a track finished playing, naturally or via stop. An unknown id
  /// reads as ended: finished tracks are swept from the registry, so absence
  /// means the voice is gone.
  pub fn audio_ended(&self, id: u64) -> bool {
    match self.audio.tracks.borrow().get(&id) {
      Some(track) => !track.is_playing() && !track.is_paused(),
      None => true,
    }
  }

  /// Release a loaded sound. Any voices already playing keep going (the C
  /// library ref-counts the decoded data) until they stop on their own.
  pub fn unload_sound(&self, sound_id: u64) {
    self.audio.sounds.borrow_mut().remove(&sound_id);
  }

  /// Stop a single track, fading to silence over `fade_out_ms` first when it
  /// is > 0 (sample-accurate, SDL-applied; the track keeps playing while it
  /// fades and reads as ended once the fade completes). A no-op if it already
  /// finished. Stopping supersedes any ramps on the voice.
  pub fn stop_audio(&self, id: u64, fade_out_ms: f64) {
    self.audio.ramps.purge_tracks(&[id]);
    if fade_out_ms > 0.0 {
      // The track stays in the registry while it fades; the sweep reclaims it.
      if let Some(track) = self.audio.tracks.borrow().get(&id) {
        let frames = track.ms_to_frames(fade_out_ms as i64);
        let _ = track.stop(frames);
      }
    } else {
      // Dropping the Track destroys it (MIX_DestroyTrack), which stops playback.
      self.audio.tracks.borrow_mut().remove(&id);
    }
  }

  /// Stop every playing track, fading the whole mix out over `fade_out_ms`
  /// first when it is > 0 (fading tracks are reclaimed by the sweep).
  pub fn stop_all_audio(&self, fade_out_ms: f64) {
    self.audio.ramps.purge_all_tracks();
    if fade_out_ms > 0.0 {
      if let Some(mixer) = *self.audio.mixer.borrow() {
        let _ = mixer.stop_all(fade_out_ms as i64);
      }
    } else {
      self.audio.tracks.borrow_mut().clear();
    }
  }

  /// Stop every track on one bus (see `PlayOptions::bus`), fading it out over
  /// `fade_out_ms` first when it is > 0. Tracks are not destroyed here (SDL
  /// stops them; the sweep reclaims them), so no ramp purge is needed - live
  /// ramps on a stopped track are harmless and expire on their own.
  pub fn stop_bus_audio(&self, bus: &str, fade_out_ms: f64) {
    if let Some(mixer) = *self.audio.mixer.borrow() {
      let _ = mixer.stop_tag(bus, fade_out_ms.max(0.0) as i64);
    }
  }

  /// Release every track and loaded sound. Called between engine runs so a
  /// reloaded app never inherits (or leaks) a sound left playing or a decoded
  /// clip. The device itself stays open. PCM sinks close too (their streams
  /// are per-consumer, nothing to share across runs).
  pub fn close_all_audio(&self) {
    self.audio.ramps.purge_everything();
    self.audio.tracks.borrow_mut().clear();
    self.audio.sounds.borrow_mut().clear();
    self.audio.sinks.borrow_mut().clear();
    // The device (and its master gain) outlives runs; reset so a reloaded app
    // does not inherit a mute from the previous one.
    if let Some(mixer) = *self.audio.mixer.borrow() {
      let _ = mixer.set_gain(1.0);
    }
  }

  /// Open a streaming PCM sink: interleaved f32 at `sample_rate`/`channels`,
  /// fed with `pcm_sink_push` and played on the default output. Starts
  /// playing (an empty stream is silence); `set_pcm_sink_paused` gates it.
  /// Returns the sink id.
  pub fn create_pcm_sink(&self, sample_rate: u32, channels: u16) -> Result<u64, String> {
    if sample_rate == 0 || channels == 0 {
      return Err(format!("invalid pcm sink spec: {sample_rate} Hz x {channels} channels"));
    }
    if !sdl_utils::audio_subsystem_init() {
      return Err(format!("audio subsystem init failed: {}", sdl_utils::sdl_error()));
    }
    let stream = sdl_utils::audio_open_playback_stream(sample_rate, channels);
    if stream.is_null() {
      return Err(format!("failed to open playback stream: {}", sdl_utils::sdl_error()));
    }
    sdl_utils::audio_stream_resume(stream);
    let id = {
      let mut next = self.audio.next_sink_id.borrow_mut();
      *next += 1;
      *next
    };
    self.audio.sinks.borrow_mut().insert(id, PcmSink { stream, sample_rate, channels, pushed_frames: 0 });
    Ok(id)
  }

  /// Queue interleaved f32 samples on a sink (non-blocking, SDL buffers).
  /// The sample count must be a whole number of frames.
  pub fn pcm_sink_push(&self, id: u64, samples: &[f32]) -> Result<(), String> {
    let mut sinks = self.audio.sinks.borrow_mut();
    let sink = sinks.get_mut(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    if samples.len() % sink.channels as usize != 0 {
      return Err(format!("{} samples is not whole {}-channel frames", samples.len(), sink.channels));
    }
    if !sdl_utils::audio_stream_put_f32(sink.stream, samples) {
      return Err(format!("pcm push failed: {}", sdl_utils::sdl_error()));
    }
    sink.pushed_frames += (samples.len() / sink.channels as usize) as u64;
    Ok(())
  }

  /// The sink's playback position in microseconds: frames consumed off its
  /// queue (pushed minus still-queued) at the sink's rate. This is the
  /// master clock for A/V sync - video frames are presented against it.
  pub fn pcm_sink_position_us(&self, id: u64) -> Result<i64, String> {
    let sinks = self.audio.sinks.borrow();
    let sink = sinks.get(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    let queued_frames = sdl_utils::audio_stream_queued_bytes(sink.stream).max(0) as u64 / (4 * sink.channels as u64);
    let consumed = sink.pushed_frames.saturating_sub(queued_frames);
    Ok((consumed as i128 * 1_000_000 / sink.sample_rate as i128) as i64)
  }

  /// Microseconds of audio queued and not yet consumed - the pusher's
  /// backpressure signal (stop pushing above a lookahead threshold).
  pub fn pcm_sink_queued_us(&self, id: u64) -> Result<i64, String> {
    let sinks = self.audio.sinks.borrow();
    let sink = sinks.get(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    let queued_frames = sdl_utils::audio_stream_queued_bytes(sink.stream).max(0) as u64 / (4 * sink.channels as u64);
    Ok((queued_frames as i128 * 1_000_000 / sink.sample_rate as i128) as i64)
  }

  /// Pause or resume consumption. Paused, the queue holds and the position
  /// freezes - which is exactly what pauses video against the audio clock.
  pub fn set_pcm_sink_paused(&self, id: u64, paused: bool) -> Result<(), String> {
    let sinks = self.audio.sinks.borrow();
    let sink = sinks.get(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    let ok =
      if paused { sdl_utils::audio_stream_pause(sink.stream) } else { sdl_utils::audio_stream_resume(sink.stream) };
    if ok {
      Ok(())
    } else {
      Err(format!("pcm sink pause failed: {}", sdl_utils::sdl_error()))
    }
  }

  /// Scale the sink's volume (1.0 = unchanged), applied by SDL at mix time.
  pub fn set_pcm_sink_gain(&self, id: u64, gain: f32) -> Result<(), String> {
    let sinks = self.audio.sinks.borrow();
    let sink = sinks.get(&id).ok_or_else(|| format!("pcm sink {id} not found"))?;
    if sdl_utils::audio_stream_set_gain(sink.stream, gain.max(0.0)) {
      Ok(())
    } else {
      Err(format!("pcm sink gain failed: {}", sdl_utils::sdl_error()))
    }
  }

  /// Close a sink and its stream (which closes the device binding). Queued
  /// audio is dropped, not drained - closing is a stop, not a fade-out.
  pub fn destroy_pcm_sink(&self, id: u64) {
    self.audio.sinks.borrow_mut().remove(&id);
  }
}
