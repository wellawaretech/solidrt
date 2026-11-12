// The mixer's device side: ore's playback device, pulling on its audio
// thread, where the engine renders; and the per-voice converter (ore's
// unbound converter stream, used as resampler and channel converter).
// Opened once, kept for the process lifetime: the device outlives app runs,
// as the SDL_mixer device did. ore's Android resume check reopens the
// device stream on the same mixer state when the platform leaves it dead.
//
// The render callback runs inside the device's pull and puts into it. It
// takes the engine's mutex, which only it ever locks, and tries the command
// and retired vectors' locks: a miss defers them to the next block rather
// than blocking the audio thread on the control thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::engine::{Command, Converter, Engine, EngineStats, Voice, SCRATCH_FRAMES};

/// Every source the mixer plays is mono or stereo.
const MAX_SOURCE_CHANNELS: usize = 2;
/// The mix is stereo.
const OUTPUT_CHANNELS: u16 = 2;

/// Everything the render callback and the control thread share about the
/// open device. Held by the callback for the process lifetime.
pub(crate) struct Shared {
  pub(crate) device_rate: u32,
  /// The device's buffer size in frames, as the platform reported it at
  /// open.
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
  /// `queued`, the device stream's queue the caller read (frames rendered
  /// and not yet consumed by the device: how far positions lead the speaker
  /// before the hardware's own latency).
  pub(crate) fn stats(&self, queued: u64) -> (EngineStats, u64, u64) {
    let stats = self.engine.lock().expect("audio engine lock poisoned").stats();
    (stats, self.render_max_nanos.swap(0, Ordering::AcqRel), queued)
  }

  // The callback's body: apply what the control thread sent, render what
  // the device asks for in scratch-sized passes, hand back the retired.
  fn pull(&self, pull: &mut ore::audio::Pull<'_>) {
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
    let lead = pull.lead_frames();
    let mut remaining = pull.frames();
    while remaining > 0 {
      let frames = remaining.min(SCRATCH_FRAMES);
      let block = &mut mix[..frames * OUTPUT_CHANNELS as usize];
      engine.render(block, now, lead);
      pull.put(block);
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

/// The open device: the shared state the render callback holds, and ore's
/// playback device pulling it.
pub(crate) struct Device {
  pub(crate) shared: Arc<Shared>,
  pub(crate) playback: ore::audio::Playback,
}

impl Device {
  /// Open the default playback device at its own rate and start pulling.
  pub(crate) fn open() -> Result<Device, String> {
    let (device_rate, device_frames) = ore::audio::default_playback_format()?;
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
    });
    let playback = ore::audio::Playback::open(OUTPUT_CHANNELS, {
      let shared = Arc::clone(&shared);
      move |pull| shared.pull(pull)
    })?;
    Ok(Device { shared, playback })
  }

  /// A converter from a source's (rate, channels) to the device's stereo.
  pub(crate) fn converter(&self, sample_rate: u32, channels: u16) -> Result<Box<dyn Converter>, String> {
    let converter = ore::audio::Converter::new((sample_rate, channels), (self.shared.device_rate, OUTPUT_CHANNELS))?;
    Ok(Box::new(converter))
  }
}

/// ore's unbound converter stream as a voice's resampler. Created and
/// dropped on the control thread; put/get/flush on the audio thread (only
/// the audio thread touches a live one).
impl Converter for ore::audio::Converter {
  fn put(&mut self, samples: &[f32]) {
    ore::audio::Converter::put(self, samples);
  }

  fn flush(&mut self) {
    ore::audio::Converter::flush(self);
  }

  fn available(&self) -> usize {
    ore::audio::Converter::available(self)
  }

  fn get(&mut self, out: &mut [f32]) -> usize {
    ore::audio::Converter::get(self, out)
  }

  fn set_ratio(&mut self, ratio: f32) {
    ore::audio::Converter::set_ratio(self, ratio);
  }

  fn clear(&mut self) {
    ore::audio::Converter::clear(self);
  }
}
