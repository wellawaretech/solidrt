// The mixer's realtime core: a fixed table of voices rendered into one
// stereo block at the device rate, with sample-accurate gain automation.
// Nothing here knows SDL (the device and the resampler are behind
// `Converter`), nothing here allocates, locks or logs: the device callback
// calls `apply` then `render` with the stream's lock held, and a test
// drives the same two calls with an identity converter.
//
// Per block: the master and every bus in use step their gain ramps into a
// curve (once per sample, whatever the voice count); each live voice feeds
// its converter from its source until the block's frames are available (a
// looping clip wraps while feeding, so the resampler interpolates across
// the seam), takes the converted stereo frames, and accumulates them with
// its own gain and envelope ramps per sample, its pan gains interpolated
// across the block, and the curves. A voice retires when its envelope
// fades out, when its source is exhausted and its converter drained, or on
// a stop; retired voices go back to the control thread to be freed there.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::ring::RingConsumer;

/// Live-voice ceiling: the voice table has this many slots. Far above any
/// real mix (real mixes run dozens); the cap turns a runaway play() loop
/// into an error instead of a wedged process.
pub const MAX_LIVE_TRACKS: usize = 256;
/// Slots in the voice table: the cap plus as much again for voices stopped
/// but not yet retired by the callback (a stop frees capacity at once on
/// the control side; the slot itself returns a block later).
pub(crate) const SLOT_TABLE: usize = MAX_LIVE_TRACKS * 2;
/// Named buses an app can use at once; names are created by use.
pub const MAX_BUSES: usize = 32;
/// The most frames one `render` call takes: the scratch buffers' size. A
/// device asking for more is served in passes.
pub(crate) const SCRATCH_FRAMES: usize = 1024;
/// Source frames handed to a converter per put while it is short of a
/// block: a few puts per block at most, each one copy.
const FEED_FRAMES: usize = 512;
/// The resampler's ratio range (SDL_SetAudioStreamFrequencyRatio).
const MIN_RATE: f32 = 0.01;
const MAX_RATE: f32 = 100.0;

/// Map a pan position in [-1, 1] (clamped) to per-channel gains using the
/// equal-power law, so perceived loudness stays constant as a sound sweeps
/// across the field. Center is (0.707, 0.707), the same 3 dB dip a Web Audio
/// StereoPannerNode applies to a mono source at pan 0.
pub(crate) fn pan_gains(pan: f32) -> (f32, f32) {
  // clamp propagates NaN; read NaN as center so the gains are always valid.
  let pan = if pan.is_nan() { 0.0 } else { pan.clamp(-1.0, 1.0) };
  let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
  (angle.cos(), angle.sin())
}

/// Clamp a playback rate to the range the resampler supports. NaN reads as
/// 1.0 (unchanged), like pan_gains reads NaN as center.
pub(crate) fn clamp_rate(rate: f32) -> f32 {
  if rate.is_nan() {
    1.0
  } else {
    rate.clamp(MIN_RATE, MAX_RATE)
  }
}

/// Whether every f32 sample is finite. Non-finite samples are the signature
/// of a synthesis bug (a NaN poisons the mix), so loading rejects them
/// instead of playing a pop.
pub(crate) fn pcm_all_finite(samples: &[f32]) -> bool {
  samples.iter().all(|s| s.is_finite())
}

/// A decoded clip: interleaved f32 at its own rate. Shared between the
/// registry (until unloaded) and every voice playing it.
pub struct Pcm {
  pub sample_rate: u32,
  pub channels: u16,
  pub samples: Vec<f32>,
}

impl Pcm {
  pub fn frames(&self) -> usize {
    self.samples.len() / self.channels as usize
  }
}

/// A linear parameter ramp in frames: `set` aims at a target over a frame
/// count, `advance` steps one frame. Sample-accurate automation for gains
/// and the envelope; block-stepped (`advance_by`) for the block parameters.
pub(crate) struct Ramp {
  value: f32,
  target: f32,
  step: f32,
  remaining: u32,
}

impl Ramp {
  pub(crate) fn at(value: f32) -> Ramp {
    Ramp { value, target: value, step: 0.0, remaining: 0 }
  }

  /// Head for `target` over `frames` (0 = jump now). A new set starts from
  /// the current value, replacing any ramp in flight.
  pub(crate) fn set(&mut self, target: f32, frames: u32) {
    self.target = target;
    if frames == 0 {
      self.value = target;
      self.remaining = 0;
      self.step = 0.0;
    } else {
      self.remaining = frames;
      self.step = (target - self.value) / frames as f32;
    }
  }

  pub(crate) fn current(&self) -> f32 {
    self.value
  }

  pub(crate) fn done(&self) -> bool {
    self.remaining == 0
  }

  /// One frame on: the value for this frame.
  #[inline]
  pub(crate) fn advance(&mut self) -> f32 {
    if self.remaining > 0 {
      self.remaining -= 1;
      self.value = if self.remaining == 0 { self.target } else { self.value + self.step };
    }
    self.value
  }

  /// `frames` frames on: the value after them.
  pub(crate) fn advance_by(&mut self, frames: usize) -> f32 {
    if self.remaining > 0 {
      let n = (frames as u32).min(self.remaining);
      self.remaining -= n;
      self.value = if self.remaining == 0 { self.target } else { self.value + self.step * n as f32 };
    }
    self.value
  }
}

/// The resampler and channel converter in front of a voice: source frames
/// (the clip's rate and channels) in, stereo frames at the device rate
/// out, at a variable ratio. SDL's audio stream in the device; an identity
/// converter in tests.
pub(crate) trait Converter: Send {
  /// Queue interleaved source frames.
  fn put(&mut self, samples: &[f32]);
  /// No more input is coming: make everything buffered available.
  fn flush(&mut self);
  /// Stereo frames ready to `get` (an estimate while resampling, exact
  /// after a flush).
  fn available(&self) -> usize;
  /// Take converted stereo frames into `out`; the frames written.
  fn get(&mut self, out: &mut [f32]) -> usize;
  /// The playback rate: >1 consumes input faster (higher pitch).
  fn set_ratio(&mut self, ratio: f32);
  /// Drop everything queued and buffered.
  fn clear(&mut self);
}

/// What a voice reads from.
pub(crate) enum Source {
  /// A decoded clip and the next frame to feed.
  Clip { pcm: Arc<Pcm>, cursor: usize },
  /// A ring the decode thread fills from a decoder; `frames` is the clip's
  /// length when known (positions wrap on it while looping). Nothing is
  /// read before the ring's generation reaches `generation`: the restart
  /// this voice was started for has then been applied.
  Stream { ring: RingConsumer, frames: Option<u64>, generation: u32 },
  /// A ring any one thread pushes into: never ends, silent when empty.
  Sink { ring: RingConsumer },
}

/// What came of asking a source for frames.
enum Feed {
  Frames(usize),
  /// Nothing now, more may come (a starved ring).
  Starved,
  /// Nothing now, by the pusher's word (a sink's ring ended): silence,
  /// not starvation; the sink stays.
  Quiet,
  /// Nothing more, ever (clip end, ring ended).
  Exhausted,
}

/// The voice's state readers on other threads see, published per block.
pub struct VoiceShared {
  /// The voice has left the mix (finished, stopped or faded out).
  pub(crate) ended: AtomicBool,
  /// Paused (its own flag or its bus's) as of the last block.
  pub(crate) paused: AtomicBool,
  /// Frames of the source heard so far: the cursor less the converter's
  /// backlog, wrapped on the clip length for a looping clip.
  pub(crate) position: AtomicU64,
  /// Source frames fed to the converter and not yet played by the device
  /// (the converter's backlog plus the device queue's share), as of the
  /// last block: what a ring's consumed index is ahead of the speaker by.
  pub(crate) backlog: AtomicU64,
  /// For interpolating a position between blocks: the position above, the
  /// nanoseconds (on the engine's clock) the block was rendered at and the
  /// block's length in source frames, under a seqlock.
  pub(crate) seq: AtomicU32,
  pub(crate) at_nanos: AtomicU64,
  pub(crate) block_frames: AtomicU64,
}

impl VoiceShared {
  pub(crate) fn new() -> Arc<VoiceShared> {
    Arc::new(VoiceShared {
      ended: AtomicBool::new(false),
      paused: AtomicBool::new(false),
      position: AtomicU64::new(0),
      backlog: AtomicU64::new(0),
      seq: AtomicU32::new(0),
      at_nanos: AtomicU64::new(0),
      block_frames: AtomicU64::new(0),
    })
  }

  pub fn ended(&self) -> bool {
    self.ended.load(Ordering::Acquire)
  }

  pub fn paused(&self) -> bool {
    self.paused.load(Ordering::Acquire)
  }

  /// The position as of the last block, in source frames.
  pub fn position(&self) -> u64 {
    self.position.load(Ordering::Acquire)
  }

  /// The last block's (position, rendered-at nanos, length in source
  /// frames), read consistently.
  pub fn block(&self) -> (u64, u64, u64) {
    loop {
      let before = self.seq.load(Ordering::Acquire);
      let position = self.position.load(Ordering::Acquire);
      let at = self.at_nanos.load(Ordering::Acquire);
      let frames = self.block_frames.load(Ordering::Acquire);
      let after = self.seq.load(Ordering::Acquire);
      if before == after && before % 2 == 0 {
        return (position, at, frames);
      }
    }
  }

  fn publish(&self, position: u64, backlog: u64, at_nanos: u64, block_frames: u64) {
    self.seq.fetch_add(1, Ordering::AcqRel);
    self.position.store(position, Ordering::Release);
    self.backlog.store(backlog, Ordering::Release);
    self.at_nanos.store(at_nanos, Ordering::Release);
    self.block_frames.store(block_frames, Ordering::Release);
    self.seq.fetch_add(1, Ordering::AcqRel);
  }
}

/// A slot in the voice table plus the generation of its occupant, so a
/// command aimed at a voice that has since retired and been replaced hits
/// nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Slot {
  pub(crate) index: u32,
  pub(crate) generation: u32,
}

/// One voice: its source, its converter, its automation and its state.
/// Built on the control thread (allocations happen there), moved into the
/// engine by `Command::Start`, moved back out when retired.
pub(crate) struct Voice {
  pub(crate) slot: Slot,
  pub(crate) source: Source,
  pub(crate) converter: Box<dyn Converter>,
  pub(crate) channels: usize,
  /// Source frames per device frame at ratio 1 (source rate / device rate).
  pub(crate) rate_scale: f64,
  pub(crate) gain: Ramp,
  /// None: unspatialized (no pan processing at all).
  pub(crate) pan: Option<Ramp>,
  pub(crate) rate: Ramp,
  /// The fade-in/fade-out envelope, apart from the user gain.
  pub(crate) envelope: Ramp,
  /// The envelope reaching its target ends the voice (a fade-out stop).
  pub(crate) ends_on_envelope: bool,
  pub(crate) bus: Option<usize>,
  pub(crate) looping: bool,
  pub(crate) paused: bool,
  pub(crate) shared: Arc<VoiceShared>,
  // Bookkeeping for positions: source frames fed, and heard (fed less the
  // converter's backlog, accumulated from output frames at the ratio).
  fed: u64,
  heard: f64,
  // Frames a ring discard skipped (count as consumed, never fed).
  skipped: u64,
  // The sink ring's generation last seen: a change is a clear.
  ring_generation: u32,
  last_ratio: f32,
  // The source is exhausted and the converter flushed: playing out.
  draining: bool,
  // A ring that has had data: an empty block is an underrun, not a wait.
  had_data: bool,
}

impl Voice {
  pub(crate) fn new(
    slot: Slot,
    source: Source,
    converter: Box<dyn Converter>,
    channels: u16,
    sample_rate: u32,
    device_rate: u32,
    shared: Arc<VoiceShared>,
  ) -> Voice {
    Voice {
      slot,
      source,
      converter,
      channels: channels as usize,
      rate_scale: sample_rate as f64 / device_rate as f64,
      gain: Ramp::at(1.0),
      pan: None,
      rate: Ramp::at(1.0),
      envelope: Ramp::at(1.0),
      ends_on_envelope: false,
      bus: None,
      looping: false,
      paused: false,
      shared,
      fed: 0,
      heard: 0.0,
      skipped: 0,
      ring_generation: 0,
      last_ratio: 1.0,
      draining: false,
      had_data: false,
    }
  }

  /// Fill `feed` with up to FEED_FRAMES source frames.
  fn feed(&mut self, feed: &mut [f32]) -> Feed {
    let channels = self.channels;
    match &mut self.source {
      Source::Clip { pcm, cursor } => {
        let total = pcm.frames();
        if *cursor >= total {
          if self.looping && total > 0 {
            *cursor = 0;
          } else {
            return Feed::Exhausted;
          }
        }
        let frames = FEED_FRAMES.min(total - *cursor);
        let start = *cursor * channels;
        feed[..frames * channels].copy_from_slice(&pcm.samples[start..start + frames * channels]);
        *cursor += frames;
        Feed::Frames(frames)
      }
      Source::Stream { ring, generation, .. } => {
        if ring.shared().generation() < *generation {
          return Feed::Starved;
        }
        self.skipped += ring.apply_discard();
        let frames = ring.pop(&mut feed[..FEED_FRAMES * channels]);
        if frames > 0 {
          self.had_data = true;
          Feed::Frames(frames)
        } else if ring.shared().ended() {
          Feed::Exhausted
        } else {
          Feed::Starved
        }
      }
      Source::Sink { ring } => {
        self.skipped += ring.apply_discard();
        let frames = ring.pop(&mut feed[..FEED_FRAMES * channels]);
        if frames > 0 {
          self.had_data = true;
          Feed::Frames(frames)
        } else if ring.shared().ended() {
          Feed::Quiet
        } else {
          Feed::Starved
        }
      }
    }
  }

  /// A sink's clear, before anything else in the block (whether or not the
  /// block needs to feed): what the converter holds is the cleared audio
  /// too, and all of it counts as consumed.
  fn apply_clear(&mut self) {
    if let Source::Sink { ring } = &self.source {
      let generation = ring.shared().generation();
      if generation != self.ring_generation {
        self.ring_generation = generation;
        self.converter.clear();
        self.heard = self.fed as f64;
      }
    }
  }

  /// The position to publish: source frames heard, in the source's own
  /// frame space (a clip's offset, a stream's content frame, a sink's
  /// consumed count). `lead` is the source frames rendered ahead of the
  /// device (the device stream's queue), so the position is the device's
  /// consumption, not the mix's.
  fn position(&self, lead: f64) -> u64 {
    let heard = (self.heard - lead).max(0.0) as u64;
    match &self.source {
      Source::Clip { pcm, .. } => {
        let total = pcm.frames() as u64;
        if self.looping && total > 0 {
          heard % total
        } else {
          heard.min(total)
        }
      }
      Source::Stream { ring, frames, .. } => {
        let backlog = self.fed.saturating_sub(heard);
        let content = ring.shared().content_at(ring.read_index()).saturating_sub(backlog);
        match frames {
          Some(total) if self.looping && *total > 0 => content % total,
          _ => content,
        }
      }
      Source::Sink { .. } => self.skipped + heard,
    }
  }
}

/// A named bus: a gain ramp every voice on it multiplies by, a pause flag,
/// and how many voices it has (its curve is only stepped while it has any).
struct Bus {
  gain: Ramp,
  paused: bool,
  voices: usize,
}

/// What the control thread asks of the engine, applied at the start of the
/// next block in order.
pub(crate) enum Command {
  Start(Box<Voice>),
  SetGain {
    slot: Slot,
    gain: f32,
    frames: u32,
  },
  /// A pan set on an unpanned voice sweeps from center.
  SetPan {
    slot: Slot,
    pan: f32,
    frames: u32,
  },
  SetRate {
    slot: Slot,
    rate: f32,
    frames: u32,
  },
  SetLoop {
    slot: Slot,
    looping: bool,
  },
  SetPaused {
    slot: Slot,
    paused: bool,
  },
  /// Stop a voice: at once (0 frames), or fading its envelope out first.
  Stop {
    slot: Slot,
    fade_frames: u32,
  },
  SetBusGain {
    bus: usize,
    gain: f32,
    frames: u32,
  },
  SetBusPaused {
    bus: usize,
    paused: bool,
  },
  StopBus {
    bus: usize,
    fade_frames: u32,
  },
  SetMasterGain {
    gain: f32,
    frames: u32,
  },
}

/// Counters the callback keeps; read (and the maximum reset) by stats.
pub struct EngineStats {
  pub live_voices: usize,
  pub underruns: u64,
}

pub(crate) struct Engine {
  slots: Vec<Option<Box<Voice>>>,
  buses: Vec<Bus>,
  master: Ramp,
  master_curve: Vec<f32>,
  bus_curves: Vec<Vec<f32>>,
  feed: Vec<f32>,
  voice_out: Vec<f32>,
  /// Voices that left the mix, waiting to go back to the control thread.
  retired: Vec<Box<Voice>>,
  underruns: u64,
  live: usize,
}

impl Engine {
  /// An engine for sources of up to `max_channels` channels.
  pub(crate) fn new(max_channels: usize) -> Engine {
    Engine {
      slots: (0..SLOT_TABLE).map(|_| None).collect(),
      buses: (0..MAX_BUSES).map(|_| Bus { gain: Ramp::at(1.0), paused: false, voices: 0 }).collect(),
      master: Ramp::at(1.0),
      master_curve: vec![1.0; SCRATCH_FRAMES],
      bus_curves: (0..MAX_BUSES).map(|_| vec![1.0; SCRATCH_FRAMES]).collect(),
      feed: vec![0.0; FEED_FRAMES * max_channels],
      voice_out: vec![0.0; SCRATCH_FRAMES * 2],
      retired: Vec::with_capacity(SLOT_TABLE),
      underruns: 0,
      live: 0,
    }
  }

  pub(crate) fn stats(&self) -> EngineStats {
    EngineStats { live_voices: self.live, underruns: self.underruns }
  }

  /// Apply commands in order, draining the vector.
  pub(crate) fn apply(&mut self, commands: &mut Vec<Command>) {
    for command in commands.drain(..) {
      match command {
        Command::Start(voice) => {
          let index = voice.slot.index as usize;
          if let Some(bus) = voice.bus {
            self.buses[bus].voices += 1;
          }
          if let Some(previous) = self.slots[index].replace(voice) {
            // Cannot happen (the control thread hands out free slots only);
            // never leak a voice if it does.
            self.retire_voice(previous);
          } else {
            self.live += 1;
          }
        }
        Command::SetGain { slot, gain, frames } => {
          if let Some(voice) = self.voice(slot) {
            voice.gain.set(gain, frames);
          }
        }
        Command::SetPan { slot, pan, frames } => {
          if let Some(voice) = self.voice(slot) {
            voice.pan.get_or_insert_with(|| Ramp::at(0.0)).set(pan, frames);
          }
        }
        Command::SetRate { slot, rate, frames } => {
          if let Some(voice) = self.voice(slot) {
            voice.rate.set(rate, frames);
          }
        }
        Command::SetLoop { slot, looping } => {
          if let Some(voice) = self.voice(slot) {
            voice.looping = looping;
          }
        }
        Command::SetPaused { slot, paused } => {
          if let Some(voice) = self.voice(slot) {
            voice.paused = paused;
          }
        }
        Command::Stop { slot, fade_frames } => {
          if self.voice(slot).is_some() {
            self.stop(slot.index as usize, fade_frames);
          }
        }
        Command::SetBusGain { bus, gain, frames } => self.buses[bus].gain.set(gain, frames),
        Command::SetBusPaused { bus, paused } => self.buses[bus].paused = paused,
        Command::StopBus { bus, fade_frames } => {
          for index in 0..SLOT_TABLE {
            if self.slots[index].as_ref().is_some_and(|v| v.bus == Some(bus)) {
              self.stop(index, fade_frames);
            }
          }
        }
        Command::SetMasterGain { gain, frames } => self.master.set(gain, frames),
      }
    }
  }

  fn voice(&mut self, slot: Slot) -> Option<&mut Voice> {
    self.slots[slot.index as usize].as_deref_mut().filter(|v| v.slot == slot)
  }

  fn stop(&mut self, index: usize, fade_frames: u32) {
    if fade_frames == 0 {
      if let Some(voice) = self.slots[index].take() {
        self.live -= 1;
        self.retire_voice(voice);
      }
    } else if let Some(voice) = self.slots[index].as_deref_mut() {
      voice.envelope.set(0.0, fade_frames);
      voice.ends_on_envelope = true;
    }
  }

  fn retire_voice(&mut self, voice: Box<Voice>) {
    if let Some(bus) = voice.bus {
      self.buses[bus].voices -= 1;
    }
    voice.shared.ended.store(true, Ordering::Release);
    self.retired.push(voice);
  }

  /// Move the retired voices into `into` (which the control thread frees).
  pub(crate) fn take_retired(&mut self, into: &mut Vec<Box<Voice>>) {
    into.append(&mut self.retired);
  }

  /// Render `out.len() / 2` stereo frames (at most SCRATCH_FRAMES) at
  /// `now_nanos` on the engine's clock, replacing `out`. `lead_frames` is
  /// how many device frames the device stream holds rendered and not yet
  /// consumed, this block excluded: positions are published net of it, so
  /// they mean what the device has played.
  pub(crate) fn render(&mut self, out: &mut [f32], now_nanos: u64, lead_frames: u64) {
    let frames = (out.len() / 2).min(SCRATCH_FRAMES);
    let out = &mut out[..frames * 2];
    out.fill(0.0);
    for value in &mut self.master_curve[..frames] {
      *value = self.master.advance();
    }
    for (bus, curve) in self.buses.iter_mut().zip(&mut self.bus_curves) {
      if bus.voices > 0 {
        for value in &mut curve[..frames] {
          *value = bus.gain.advance();
        }
      }
    }
    for index in 0..SLOT_TABLE {
      let Some(mut voice) = self.slots[index].take() else { continue };
      let keep = self.render_voice(&mut voice, out, frames, now_nanos, lead_frames);
      if keep {
        self.slots[index] = Some(voice);
      } else {
        self.live -= 1;
        self.retire_voice(voice);
      }
    }
  }

  /// Mix one voice into `out`; false when it is done.
  fn render_voice(
    &mut self,
    voice: &mut Voice,
    out: &mut [f32],
    frames: usize,
    now_nanos: u64,
    lead_frames: u64,
  ) -> bool {
    voice.apply_clear();
    let bus_paused = voice.bus.is_some_and(|b| self.buses[b].paused);
    let paused = voice.paused || bus_paused;
    voice.shared.paused.store(paused, Ordering::Release);
    if paused {
      return true;
    }
    let ratio = clamp_rate(voice.rate.advance_by(frames));
    if ratio != voice.last_ratio {
      voice.converter.set_ratio(ratio);
      voice.last_ratio = ratio;
    }
    // Feed until the block is covered, the source is dry, or it ended.
    while !voice.draining && voice.converter.available() < frames {
      match voice.feed(&mut self.feed) {
        Feed::Frames(n) => {
          voice.converter.put(&self.feed[..n * voice.channels]);
          voice.fed += n as u64;
        }
        Feed::Starved => {
          // A ring that has played is being outrun; one still filling for
          // its first frames is not an underrun.
          if voice.had_data {
            self.underruns += 1;
          }
          break;
        }
        Feed::Quiet => break,
        Feed::Exhausted => {
          voice.converter.flush();
          voice.draining = true;
        }
      }
    }
    let got = voice.converter.get(&mut self.voice_out[..frames * 2]);
    // Pan gains at the block's ends, interpolated across it.
    let (l0, r0, l1, r1) = match &mut voice.pan {
      Some(pan) => {
        let (l0, r0) = pan_gains(pan.current());
        let (l1, r1) = pan_gains(pan.advance_by(got));
        (l0, r0, l1, r1)
      }
      None => (1.0, 1.0, 1.0, 1.0),
    };
    let bus_curve = voice.bus.map(|b| &self.bus_curves[b]);
    let step = if got > 1 { 1.0 / (got - 1) as f32 } else { 0.0 };
    for k in 0..got {
      let mut g = voice.gain.advance() * voice.envelope.advance() * self.master_curve[k];
      if let Some(curve) = bus_curve {
        g *= curve[k];
      }
      let t = k as f32 * step;
      let gl = l0 + (l1 - l0) * t;
      let gr = r0 + (r1 - r0) * t;
      out[2 * k] += self.voice_out[2 * k] * g * gl;
      out[2 * k + 1] += self.voice_out[2 * k + 1] * g * gr;
    }
    let block_src = got as f64 * voice.rate_scale * ratio as f64;
    voice.heard += block_src;
    // The device asked for this block to play it now; what it holds from
    // earlier blocks plays first. Played = fed less the converter's backlog
    // less that queue.
    let lead_src = lead_frames as f64 * voice.rate_scale * ratio as f64;
    let position = voice.position(lead_src);
    let backlog = voice.fed.saturating_sub((voice.heard - lead_src).max(0.0) as u64);
    voice.shared.publish(position, backlog, now_nanos, block_src as u64);
    if voice.ends_on_envelope && voice.envelope.done() {
      return false;
    }
    // Drained: the converter gave less than a block and holds nothing.
    !(voice.draining && got < frames && voice.converter.available() == 0)
  }
}
