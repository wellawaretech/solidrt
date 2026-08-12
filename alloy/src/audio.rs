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

use std::cell::RefCell;
use std::collections::HashMap;

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
  /// fire-and-forget sound. A track that is playing or paused is kept.
  fn sweep_finished(&self) {
    self.tracks.borrow_mut().retain(|_, t| t.is_playing() || t.is_paused());
  }

  /// Start a fresh voice for an already-decoded clip and retain it, returning
  /// the track id. Shared by the fire-and-forget path and `play_sound`.
  fn spawn_track(
    &self,
    mixer: &'static Mixer,
    audio: &Audio,
    looping: bool,
    gain: f32,
    pan: Option<f32>,
  ) -> Result<u64, String> {
    let track = mixer.create_track().map_err(|e| format!("failed to create audio track: {e}"))?;
    track.set_audio(audio).map_err(|e| format!("failed to assign audio: {e}"))?;
    track.set_gain(gain).map_err(|e| format!("failed to set gain: {e}"))?;
    if let Some(pan) = pan {
      track.set_stereo(Some(pan_gains(pan))).map_err(|e| format!("failed to set pan: {e}"))?;
    }
    // MIX_PlayTrack resets the loop count from its play options every call, so a
    // prior set_loops is ignored (see MIX_PROP_PLAY_LOOPS_NUMBER). Pass the loop
    // count through the play options instead. -1 loops forever.
    if looping {
      let opts = Properties::new().map_err(|e| format!("failed to alloc play options: {e:?}"))?;
      opts.set("SDL_mixer.play.loops", -1i64).map_err(|e| format!("failed to set loop option: {e:?}"))?;
      track.play_with_options(&opts).map_err(|e| format!("failed to play audio: {e}"))?;
    } else {
      track.play().map_err(|e| format!("failed to play audio: {e}"))?;
    }
    let id = {
      let mut next = self.next_id.borrow_mut();
      *next += 1;
      *next
    };
    self.tracks.borrow_mut().insert(id, track);
    Ok(id)
  }
}

impl crate::context::Context {
  /// Decode and start an encoded audio clip (Ogg/Vorbis or WAV). `looping`
  /// repeats it until stopped; `gain` scales volume (1.0 = unchanged); `pan`
  /// positions it in the stereo field (see `pan_gains`), `None` leaves the clip
  /// unspatialized. Returns the track id, usable with `stop_audio`.
  pub fn play_audio(&self, bytes: &[u8], looping: bool, gain: f32, pan: Option<f32>) -> Result<u64, String> {
    self.audio.sweep_finished();
    let mixer = self.audio.mixer()?;
    // predecode=true fully decodes into the Audio at load time, so neither the
    // IOStream nor `bytes` needs to outlive this call.
    let io = IOStream::from_bytes(bytes).map_err(|e| format!("audio read failed: {e}"))?;
    let audio = mixer.load_audio_io(&io, true).map_err(|e| format!("audio decode failed: {e}"))?;
    // The mixer ref-counts audio assigned to a track, so dropping `audio` at the
    // end of this call is safe: the track keeps the decoded data alive.
    self.audio.spawn_track(mixer, &audio, looping, gain, pan)
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
  pub fn load_pcm_sound(&self, bytes: &[u8], sample_rate: i32, channels: i32, format: PcmFormat) -> Result<u64, String> {
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
  pub fn play_sound(&self, sound_id: u64, looping: bool, gain: f32, pan: Option<f32>) -> Result<u64, String> {
    self.audio.sweep_finished();
    let mixer = self.audio.mixer()?;
    let sounds = self.audio.sounds.borrow();
    let sound = sounds.get(&sound_id).ok_or_else(|| format!("unknown sound {sound_id}"))?;
    self.audio.spawn_track(mixer, &sound.audio, looping, gain, pan)
  }

  /// Change the gain of a playing track. A voice that already finished (or was
  /// stopped) is silently skipped: live control races with natural completion
  /// by design, so a late set is not an error.
  pub fn set_audio_gain(&self, id: u64, gain: f32) -> Result<(), String> {
    match self.audio.tracks.borrow().get(&id) {
      Some(track) => track.set_gain(gain).map_err(|e| format!("failed to set gain: {e}")),
      None => Ok(()),
    }
  }

  /// Position a playing track in the stereo field, pan in [-1, 1] (clamped),
  /// 0 = center; see `pan_gains` for the law. Silently skipped for a finished
  /// voice, like `set_audio_gain`.
  pub fn set_audio_pan(&self, id: u64, pan: f32) -> Result<(), String> {
    match self.audio.tracks.borrow().get(&id) {
      Some(track) => track.set_stereo(Some(pan_gains(pan))).map_err(|e| format!("failed to set pan: {e}")),
      None => Ok(()),
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

  /// Stop and release a single track. A no-op if it already finished.
  pub fn stop_audio(&self, id: u64) {
    // Dropping the Track destroys it (MIX_DestroyTrack), which stops playback.
    self.audio.tracks.borrow_mut().remove(&id);
  }

  /// Stop and release every playing track.
  pub fn stop_all_audio(&self) {
    self.audio.tracks.borrow_mut().clear();
  }

  /// Release every track and loaded sound. Called between engine runs so a
  /// reloaded app never inherits (or leaks) a sound left playing or a decoded
  /// clip. The device itself stays open. PCM sinks close too (their streams
  /// are per-consumer, nothing to share across runs).
  pub fn close_all_audio(&self) {
    self.audio.tracks.borrow_mut().clear();
    self.audio.sounds.borrow_mut().clear();
    self.audio.sinks.borrow_mut().clear();
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
