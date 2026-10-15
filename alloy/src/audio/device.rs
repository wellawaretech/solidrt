// The mixer's SDL side: one stream bound to the default playback device,
// pulled by SDL's get callback on its audio thread, where the engine
// renders; and the per-voice converter (an unbound SDL audio stream used
// as resampler and channel converter). Opened once, kept for the process
// lifetime: the device outlives app runs, as the SDL_mixer device did.
//
// The callback holds the stream's lock while it runs and may put into that
// same stream (SDL's documented contract for a get callback). It takes the
// engine's mutex, which only it ever locks, and tries the command and
// retired vectors' locks: a miss defers them to the next block rather than
// blocking the audio thread on the control thread.

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use sdl3::sys::audio::SDL_AudioStream;

use super::engine::{Command, Converter, Engine, EngineStats, Voice, SCRATCH_FRAMES};
use crate::sdl_utils;

/// The device rate assumed when SDL cannot say (it converts to the real
/// one either way).
const FALLBACK_DEVICE_RATE: u32 = 48_000;
/// Every source the mixer plays is mono or stereo.
const MAX_SOURCE_CHANNELS: usize = 2;
/// The mix is stereo.
const OUTPUT_CHANNELS: u16 = 2;

/// Everything the callback and the control thread share about the open
/// device. Leaked into the callback's userdata for the process lifetime.
pub(crate) struct Shared {
  pub(crate) device_rate: u32,
  /// The device's buffer size in frames, as SDL reported it at open.
  pub(crate) device_frames: u32,
  engine: Mutex<Engine>,
  /// Control thread pushes, callback swaps out and applies.
  commands: Mutex<Vec<Command>>,
  /// Callback pushes, control thread drains and frees.
  retired: Mutex<Vec<Box<Voice>>>,
  /// The callback's mix scratch (stereo frames) and its pending vectors,
  /// the ones it swaps against the shared pair.
  scratch: Mutex<Scratch>,
  epoch: Instant,
  /// The longest `render` pass in nanoseconds since last read.
  render_max_nanos: AtomicU64,
  /// The device stream, for reading its queue from the control thread
  /// (thread-safe on SDL's side); set once at open.
  stream: AtomicPtr<SDL_AudioStream>,
}

struct Scratch {
  mix: Vec<f32>,
  commands: Vec<Command>,
  retired: Vec<Box<Voice>>,
}

impl Shared {
  /// Nanoseconds since the device opened: the clock positions are stamped
  /// with.
  pub(crate) fn now_nanos(&self) -> u64 {
    self.epoch.elapsed().as_nanos() as u64
  }

  pub(crate) fn nanos_of(&self, at: Instant) -> u64 {
    at.saturating_duration_since(self.epoch).as_nanos() as u64
  }

  /// Queue a command for the next block.
  pub(crate) fn send(&self, command: Command) {
    self.commands.lock().expect("audio command lock poisoned").push(command);
  }

  /// Take the voices the callback has retired, to free them here.
  pub(crate) fn take_retired(&self) -> Vec<Box<Voice>> {
    std::mem::take(&mut *self.retired.lock().expect("audio retired lock poisoned"))
  }

  /// The engine's counters, the longest render pass since last asked, and
  /// the device stream's queue (frames rendered and not yet consumed by the
  /// device: how far positions lead the speaker before the hardware's own
  /// latency).
  pub(crate) fn stats(&self) -> (EngineStats, u64, u64) {
    let stats = self.engine.lock().expect("audio engine lock poisoned").stats();
    let queued = Self::queued_frames(self.stream.load(Ordering::Acquire));
    (stats, self.render_max_nanos.swap(0, Ordering::AcqRel), queued)
  }

  /// Frames the device stream holds, rendered and unconsumed (the stream
  /// is at the device's own format, so nothing waits in conversion).
  fn queued_frames(stream: *mut SDL_AudioStream) -> u64 {
    if stream.is_null() {
      return 0;
    }
    sdl_utils::audio_stream_queued_bytes(stream).max(0) as u64 / (4 * OUTPUT_CHANNELS as u64)
  }

  // The callback's body: apply what the control thread sent, render what
  // the device asks for in scratch-sized passes, hand back the retired.
  fn pull(&self, stream: *mut SDL_AudioStream, additional_bytes: i32) {
    let started = Instant::now();
    let mut scratch = match self.scratch.try_lock() {
      Ok(scratch) => scratch,
      // Only this callback ever takes it; a miss is a reentry SDL does not do.
      Err(_) => return,
    };
    let Scratch { mix, commands, retired } = &mut *scratch;
    if let Ok(mut shared) = self.commands.try_lock() {
      std::mem::swap(&mut *shared, commands);
    }
    let mut engine = self.engine.lock().expect("audio engine lock poisoned");
    engine.apply(commands);
    let now = self.now_nanos();
    // What the device stream still holds of earlier blocks: rendered,
    // unheard. Everything rendered in this pull is what the device asked
    // for now, so it counts as played.
    let lead = Self::queued_frames(stream);
    let mut remaining = (additional_bytes.max(0) as usize) / (4 * OUTPUT_CHANNELS as usize);
    while remaining > 0 {
      let frames = remaining.min(SCRATCH_FRAMES);
      let block = &mut mix[..frames * OUTPUT_CHANNELS as usize];
      engine.render(block, now, lead);
      sdl_utils::audio_stream_put_f32(stream, block);
      remaining -= frames;
    }
    engine.take_retired(retired);
    drop(engine);
    if !retired.is_empty() {
      if let Ok(mut shared) = self.retired.try_lock() {
        shared.append(retired);
      }
    }
    let took = started.elapsed().as_nanos() as u64;
    self.render_max_nanos.fetch_max(took, Ordering::AcqRel);
  }
}

unsafe extern "C" fn on_pull(userdata: *mut c_void, stream: *mut SDL_AudioStream, additional: i32, _total: i32) {
  // The userdata is a leaked Arc<Shared>: alive for the process.
  let shared = &*(userdata as *const Shared);
  shared.pull(stream, additional);
}

/// The open device: the shared state and the SDL stream on it.
pub(crate) struct Device {
  pub(crate) shared: Arc<Shared>,
  stream: *mut SDL_AudioStream,
}

impl Device {
  /// Open the default playback device at its own rate and start pulling.
  pub(crate) fn open() -> Result<Device, String> {
    if !sdl_utils::audio_subsystem_init() {
      return Err(format!("audio subsystem init failed: {}", sdl_utils::sdl_error()));
    }
    let (device_rate, device_frames) = match sdl_utils::audio_playback_device_format() {
      Some((rate, _, frames)) if rate > 0 => (rate, frames),
      _ => (FALLBACK_DEVICE_RATE, 0),
    };
    let shared = Arc::new(Shared {
      device_rate,
      device_frames,
      engine: Mutex::new(Engine::new(MAX_SOURCE_CHANNELS)),
      commands: Mutex::new(Vec::with_capacity(super::engine::SLOT_TABLE)),
      retired: Mutex::new(Vec::with_capacity(super::engine::SLOT_TABLE)),
      scratch: Mutex::new(Scratch {
        mix: vec![0.0; SCRATCH_FRAMES * OUTPUT_CHANNELS as usize],
        commands: Vec::with_capacity(super::engine::SLOT_TABLE),
        retired: Vec::with_capacity(super::engine::SLOT_TABLE),
      }),
      epoch: Instant::now(),
      render_max_nanos: AtomicU64::new(0),
      stream: AtomicPtr::new(std::ptr::null_mut()),
    });
    // The callback's reference: never released, like the device.
    let userdata = Arc::into_raw(Arc::clone(&shared)) as *mut c_void;
    let stream = sdl_utils::audio_open_playback_stream(device_rate, OUTPUT_CHANNELS, Some(on_pull), userdata);
    if stream.is_null() {
      unsafe { drop(Arc::from_raw(userdata as *const Shared)) };
      return Err(format!("failed to open audio device: {}", sdl_utils::sdl_error()));
    }
    shared.stream.store(stream, Ordering::Release);
    if !sdl_utils::audio_stream_resume(stream) {
      log::warn!("[alloy::audio] resume device stream: {}", sdl_utils::sdl_error());
    }
    Ok(Device { shared, stream })
  }

  /// A converter from a source's (rate, channels) to the device's stereo.
  pub(crate) fn converter(&self, sample_rate: u32, channels: u16) -> Result<Box<dyn Converter>, String> {
    let stream = sdl_utils::audio_stream_create((sample_rate, channels), (self.shared.device_rate, OUTPUT_CHANNELS));
    if stream.is_null() {
      return Err(format!("failed to create audio converter: {}", sdl_utils::sdl_error()));
    }
    Ok(Box::new(SdlConverter { stream }))
  }
}

impl Drop for Device {
  fn drop(&mut self) {
    // Never reached (the registry keeps the device for the process), but a
    // dropped device must not leave SDL pulling into a freed engine.
    sdl_utils::audio_stream_destroy(self.stream);
  }
}

/// An unbound SDL audio stream as a voice's resampler. Created and
/// destroyed on the control thread; put/get/flush on the audio thread (all
/// thread-safe on SDL's side, and only the audio thread touches a live one).
struct SdlConverter {
  stream: *mut SDL_AudioStream,
}

unsafe impl Send for SdlConverter {}

impl Converter for SdlConverter {
  fn put(&mut self, samples: &[f32]) {
    sdl_utils::audio_stream_put_f32(self.stream, samples);
  }

  fn flush(&mut self) {
    sdl_utils::audio_stream_flush(self.stream);
  }

  fn available(&self) -> usize {
    sdl_utils::audio_stream_available(self.stream).max(0) as usize / (4 * OUTPUT_CHANNELS as usize)
  }

  fn get(&mut self, out: &mut [f32]) -> usize {
    let frames = out.len() / OUTPUT_CHANNELS as usize;
    let samples = sdl_utils::audio_stream_read_f32(self.stream, &mut out[..frames * OUTPUT_CHANNELS as usize]);
    samples.max(0) as usize / OUTPUT_CHANNELS as usize
  }

  fn set_ratio(&mut self, ratio: f32) {
    sdl_utils::audio_stream_set_frequency_ratio(self.stream, ratio);
  }

  fn clear(&mut self) {
    sdl_utils::audio_stream_clear(self.stream);
  }
}

impl Drop for SdlConverter {
  fn drop(&mut self) {
    sdl_utils::audio_stream_destroy(self.stream);
  }
}
