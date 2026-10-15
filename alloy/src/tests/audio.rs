use std::collections::VecDeque;
use std::sync::Arc;

use crate::audio::engine::{
  clamp_rate, pan_gains, pcm_all_finite, Command, Converter, Engine, Ramp, Slot, Source, Voice, VoiceShared,
};
use crate::audio::ring::{ring, RingConsumer};
use crate::audio::Pcm;

const EPS: f32 = 1e-6;
// Everything here runs at one rate, so the identity converter is exact.
const RATE: u32 = 48_000;

// A converter that converts nothing: mono is duplicated to stereo, the
// ratio is recorded and ignored, frames come out as they went in.
struct Identity {
  channels: usize,
  queue: VecDeque<f32>,
  ratio: f32,
}

impl Converter for Identity {
  fn put(&mut self, samples: &[f32]) {
    for frame in samples.chunks(self.channels) {
      self.queue.push_back(frame[0]);
      self.queue.push_back(frame[self.channels - 1]);
    }
  }
  fn flush(&mut self) {}
  fn available(&self) -> usize {
    self.queue.len() / 2
  }
  fn get(&mut self, out: &mut [f32]) -> usize {
    let frames = (out.len() / 2).min(self.available());
    for sample in &mut out[..frames * 2] {
      *sample = self.queue.pop_front().expect("available");
    }
    frames
  }
  fn set_ratio(&mut self, ratio: f32) {
    self.ratio = ratio;
  }
  fn clear(&mut self) {
    self.queue.clear();
  }
}

fn clip(samples: Vec<f32>, channels: u16) -> Arc<Pcm> {
  Arc::new(Pcm { sample_rate: RATE, channels, samples })
}

fn voice(index: u32, source: Source, channels: u16) -> (Box<Voice>, Arc<VoiceShared>) {
  let shared = VoiceShared::new();
  let converter = Box::new(Identity { channels: channels as usize, queue: VecDeque::new(), ratio: 1.0 });
  let slot = Slot { index, generation: 1 };
  (Box::new(Voice::new(slot, source, converter, channels, RATE, RATE, Arc::clone(&shared))), shared)
}

fn clip_voice(index: u32, samples: Vec<f32>) -> (Box<Voice>, Arc<VoiceShared>) {
  voice(index, Source::Clip { pcm: clip(samples, 1), cursor: 0 }, 1)
}

fn apply(engine: &mut Engine, commands: Vec<Command>) {
  let mut commands = commands;
  engine.apply(&mut commands);
}

fn render(engine: &mut Engine, frames: usize) -> Vec<f32> {
  let mut out = vec![0.0; frames * 2];
  engine.render(&mut out, 0, 0);
  out
}

fn left(out: &[f32]) -> Vec<f32> {
  out.iter().step_by(2).copied().collect()
}

fn assert_near(actual: f32, expected: f32, what: &str) {
  assert!((actual - expected).abs() < 1e-4, "{what}: {actual}, expected {expected}");
}

#[test]
fn pan_center_is_equal_power() {
  let (l, r) = pan_gains(0.0);
  let half = std::f32::consts::FRAC_1_SQRT_2;
  assert!((l - half).abs() < EPS);
  assert!((r - half).abs() < EPS);
}

#[test]
fn pan_extremes_are_single_channel() {
  let (l, r) = pan_gains(-1.0);
  assert!((l - 1.0).abs() < EPS);
  assert!(r.abs() < EPS);
  let (l, r) = pan_gains(1.0);
  assert!(l.abs() < EPS);
  assert!((r - 1.0).abs() < EPS);
}

#[test]
fn pan_power_is_constant() {
  for i in 0..=20 {
    let (l, r) = pan_gains(-1.0 + i as f32 * 0.1);
    let power = l * l + r * r;
    assert!((power - 1.0).abs() < 1e-5, "power {power} at step {i}");
  }
}

#[test]
fn pan_out_of_range_clamps() {
  assert_eq!(pan_gains(-5.0), pan_gains(-1.0));
  assert_eq!(pan_gains(5.0), pan_gains(1.0));
  assert_eq!(pan_gains(f32::NAN), pan_gains(0.0));
}

#[test]
fn rate_clamps_to_the_resampler_range() {
  assert_eq!(clamp_rate(2.5), 2.5);
  assert_eq!(clamp_rate(0.0), 0.01);
  assert_eq!(clamp_rate(1000.0), 100.0);
  assert_eq!(clamp_rate(f32::NAN), 1.0);
  assert_eq!(clamp_rate(f32::INFINITY), 100.0);
}

#[test]
fn pcm_finite_scan_flags_non_finite() {
  assert!(pcm_all_finite(&[0.0, -1.0, 1.0, 0.5]));
  for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
    assert!(!pcm_all_finite(&[0.0, bad, 0.0]), "{bad} should be rejected");
  }
  assert!(pcm_all_finite(&[]));
}

#[test]
fn ramp_steps_linearly_and_lands_exactly() {
  let mut ramp = Ramp::at(0.0);
  ramp.set(1.0, 4);
  let steps: Vec<f32> = (0..4).map(|_| ramp.advance()).collect();
  assert_near(steps[0], 0.25, "first");
  assert_near(steps[2], 0.75, "third");
  assert_eq!(steps[3], 1.0, "the last step is the target itself");
  assert!(ramp.done());
  assert_eq!(ramp.advance(), 1.0);
  // A block step covers several frames; a jump has no frames.
  ramp.set(0.0, 10);
  assert_near(ramp.advance_by(5), 0.5, "half");
  ramp.set(0.3, 0);
  assert_eq!(ramp.current(), 0.3);
  assert!(ramp.done());
}

#[test]
fn ring_keeps_order_across_the_wrap_and_counts_a_discard_as_consumed() {
  let (mut producer, mut consumer) = ring(8, 2);
  let frames: Vec<f32> = (0..12).map(|i| i as f32).collect();
  assert_eq!(producer.push(&frames), 6);
  let mut out = vec![0.0; 8];
  assert_eq!(consumer.pop(&mut out), 4);
  assert_eq!(out, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
  assert_eq!(producer.push(&frames[12 - 10..]), 5);
  let mut rest = vec![0.0; 14];
  assert_eq!(consumer.pop(&mut rest), 7);
  assert_eq!(&rest[..4], [8.0, 9.0, 10.0, 11.0]);
  assert_eq!(&rest[4..14], &frames[2..12]);
  assert_eq!(consumer.pop(&mut rest), 0);
  // A discard: the producer's view jumps at once, the consumer's at its
  // next pop; a push right after is kept.
  producer.push(&frames[..6]);
  producer.restart(100);
  assert_eq!(producer.shared().queued(), 0);
  assert_eq!(producer.shared().consumed(), 14);
  producer.push(&frames[..4]);
  assert_eq!(consumer.pop(&mut out), 2);
  assert_eq!(&out[..4], &frames[..4]);
  assert_eq!(consumer.shared().content_at(consumer.read_index()), 102);
}

#[test]
fn a_clip_plays_through_and_retires() {
  let mut engine = Engine::new(2);
  let samples: Vec<f32> = (1..=10).map(|i| i as f32 / 10.0).collect();
  let (voice, shared) = clip_voice(0, samples.clone());
  apply(&mut engine, vec![Command::Start(voice)]);
  assert_eq!(engine.stats().live_voices, 1);
  let out = render(&mut engine, 16);
  for k in 0..10 {
    assert_near(out[2 * k], samples[k], "left");
    assert_near(out[2 * k + 1], samples[k], "right");
  }
  assert!(out[20..].iter().all(|s| *s == 0.0), "silence after the clip");
  assert!(shared.ended());
  assert_eq!(shared.position(), 10);
  assert_eq!(engine.stats().live_voices, 0);
  let mut retired = Vec::new();
  engine.take_retired(&mut retired);
  assert_eq!(retired.len(), 1);
}

#[test]
fn gain_ramps_per_sample() {
  let mut engine = Engine::new(2);
  let (voice, _) = clip_voice(0, vec![1.0; 200]);
  let slot = voice.slot;
  apply(&mut engine, vec![Command::Start(voice), Command::SetGain { slot, gain: 0.0, frames: 100 }]);
  let out = left(&render(&mut engine, 120));
  assert_near(out[0], 0.99, "first step");
  assert_near(out[49], 0.5, "halfway");
  assert_eq!(out[99], 0.0, "lands on the target");
  assert_eq!(out[119], 0.0, "and stays");
}

#[test]
fn a_fade_out_stop_ends_the_voice_when_the_fade_lands() {
  let mut engine = Engine::new(2);
  let (voice, shared) = clip_voice(0, vec![1.0; 1000]);
  let slot = voice.slot;
  apply(&mut engine, vec![Command::Start(voice), Command::Stop { slot, fade_frames: 50 }]);
  let out = left(&render(&mut engine, 64));
  assert_near(out[0], 0.98, "fading");
  assert_eq!(out[49], 0.0, "silent at the end of the fade");
  assert!(shared.ended(), "retired after the block that finished the fade");
  let next = render(&mut engine, 8);
  assert!(next.iter().all(|s| *s == 0.0));
}

#[test]
fn a_fade_in_rises_from_silence() {
  let mut engine = Engine::new(2);
  let (mut voice, _) = clip_voice(0, vec![1.0; 100]);
  voice.envelope = Ramp::at(0.0);
  voice.envelope.set(1.0, 10);
  apply(&mut engine, vec![Command::Start(voice)]);
  let out = left(&render(&mut engine, 20));
  assert_near(out[0], 0.1, "first");
  assert_eq!(out[9], 1.0, "full");
  assert_eq!(out[15], 1.0);
}

#[test]
fn pan_places_mono_and_unpanned_passes_through() {
  let mut engine = Engine::new(2);
  let (voice, _) = clip_voice(0, vec![1.0; 100]);
  let slot = voice.slot;
  apply(&mut engine, vec![Command::Start(voice)]);
  let out = render(&mut engine, 4);
  assert_eq!((out[0], out[1]), (1.0, 1.0), "unpanned: no stereo processing");
  apply(&mut engine, vec![Command::SetPan { slot, pan: -1.0, frames: 0 }]);
  let out = render(&mut engine, 4);
  assert_near(out[0], 1.0, "hard left, left");
  assert_near(out[1], 0.0, "hard left, right");
  apply(&mut engine, vec![Command::SetPan { slot, pan: 0.0, frames: 0 }]);
  let out = render(&mut engine, 4);
  assert_near(out[0], std::f32::consts::FRAC_1_SQRT_2, "center left");
  assert_near(out[1], std::f32::consts::FRAC_1_SQRT_2, "center right");
}

#[test]
fn a_pan_ramp_sweeps_across_the_block() {
  let mut engine = Engine::new(2);
  let (voice, _) = clip_voice(0, vec![1.0; 100]);
  let slot = voice.slot;
  apply(&mut engine, vec![Command::Start(voice), Command::SetPan { slot, pan: -1.0, frames: 0 }]);
  render(&mut engine, 1);
  apply(&mut engine, vec![Command::SetPan { slot, pan: 1.0, frames: 9 }]);
  let out = render(&mut engine, 10);
  assert_near(out[0], 1.0, "starts left");
  assert_near(out[1], 0.0, "starts left");
  assert_near(out[18], 0.0, "ends right");
  assert_near(out[19], 1.0, "ends right");
  assert!(out[9] > 0.4 && out[9] < 0.6, "midway: {}", out[9]);
}

#[test]
fn a_loop_is_seamless_and_ends_after_its_pass_when_turned_off() {
  let mut engine = Engine::new(2);
  let samples: Vec<f32> = (0..10).map(|i| i as f32).collect();
  let (mut voice, shared) = clip_voice(0, samples.clone());
  voice.looping = true;
  let slot = voice.slot;
  apply(&mut engine, vec![Command::Start(voice)]);
  let out = left(&render(&mut engine, 25));
  for k in 0..25 {
    assert_eq!(out[k], samples[k % 10], "frame {k}");
  }
  assert_eq!(shared.position(), 5, "position wraps on the clip length");
  apply(&mut engine, vec![Command::SetLoop { slot, looping: false }]);
  let out = left(&render(&mut engine, 20));
  for k in 0..5 {
    assert_eq!(out[k], samples[5 + k], "the pass finishes");
  }
  assert!(out[5..].iter().all(|s| *s == 0.0), "then silence");
  assert!(shared.ended());
}

#[test]
fn voice_bus_and_master_gains_multiply() {
  let mut engine = Engine::new(2);
  let (mut voice, _) = clip_voice(0, vec![1.0; 100]);
  voice.bus = Some(3);
  let slot = voice.slot;
  apply(
    &mut engine,
    vec![
      Command::Start(voice),
      Command::SetGain { slot, gain: 0.5, frames: 0 },
      Command::SetBusGain { bus: 3, gain: 0.5, frames: 0 },
      Command::SetMasterGain { gain: 0.5, frames: 0 },
    ],
  );
  let out = render(&mut engine, 4);
  assert_near(out[0], 0.125, "0.5 x 0.5 x 0.5");
  // A bus ramp steps once per sample, whatever the voice count.
  let (second, _) = {
    let (mut v, s) = clip_voice(1, vec![1.0; 100]);
    v.bus = Some(3);
    (v, s)
  };
  apply(&mut engine, vec![Command::Start(second), Command::SetBusGain { bus: 3, gain: 1.0, frames: 10 }]);
  let out = render(&mut engine, 10);
  // Bus at step 9 of 10 = 0.95 ... the last sample lands on 1.0.
  assert_near(out[18], 0.5 * 1.0 * 0.5 + 1.0 * 1.0 * 0.5, "both voices, bus landed");
}

#[test]
fn a_paused_bus_holds_its_voices_in_place() {
  let mut engine = Engine::new(2);
  let samples: Vec<f32> = (0..100).map(|i| i as f32).collect();
  let (mut voice, shared) = clip_voice(0, samples.clone());
  voice.bus = Some(1);
  let slot = voice.slot;
  apply(&mut engine, vec![Command::Start(voice)]);
  let out = left(&render(&mut engine, 10));
  assert_eq!(out[9], 9.0);
  apply(&mut engine, vec![Command::SetBusPaused { bus: 1, paused: true }]);
  let out = render(&mut engine, 10);
  assert!(out.iter().all(|s| *s == 0.0));
  assert!(shared.paused());
  assert_eq!(shared.position(), 10);
  apply(&mut engine, vec![Command::SetBusPaused { bus: 1, paused: false }]);
  let out = left(&render(&mut engine, 10));
  assert_eq!(out[0], 10.0, "resumes where it stood");
  assert!(!shared.paused());
  // The voice's own pause does the same.
  apply(&mut engine, vec![Command::SetPaused { slot, paused: true }]);
  assert!(render(&mut engine, 10).iter().all(|s| *s == 0.0));
  apply(&mut engine, vec![Command::SetPaused { slot, paused: false }]);
  assert_eq!(left(&render(&mut engine, 1))[0], 20.0);
}

#[test]
fn commands_for_a_stale_generation_hit_nothing() {
  let mut engine = Engine::new(2);
  let (voice, _) = clip_voice(0, vec![1.0; 100]);
  let stale = Slot { index: 0, generation: 7 };
  apply(&mut engine, vec![Command::Start(voice), Command::SetGain { slot: stale, gain: 0.0, frames: 0 }]);
  assert_eq!(render(&mut engine, 1)[0], 1.0);
  apply(&mut engine, vec![Command::Stop { slot: stale, fade_frames: 0 }]);
  assert_eq!(engine.stats().live_voices, 1);
}

#[test]
fn stop_all_and_stop_bus_reach_their_voices() {
  let mut engine = Engine::new(2);
  let (mut a, sa) = clip_voice(0, vec![1.0; 100]);
  a.bus = Some(2);
  let (b, sb) = clip_voice(1, vec![1.0; 100]);
  let slot_b = b.slot;
  apply(&mut engine, vec![Command::Start(a), Command::Start(b), Command::StopBus { bus: 2, fade_frames: 0 }]);
  render(&mut engine, 1);
  assert!(sa.ended() && !sb.ended());
  apply(&mut engine, vec![Command::Stop { slot: slot_b, fade_frames: 0 }]);
  render(&mut engine, 1);
  assert!(sb.ended());
  assert_eq!(engine.stats().live_voices, 0);
}

#[test]
fn a_sink_plays_what_is_pushed_and_a_clear_counts_as_consumed() {
  let mut engine = Engine::new(2);
  let (mut producer, consumer) = ring(1000, 1);
  let ring_shared = Arc::clone(consumer.shared());
  producer.push(&[0.5; 300]);
  let (voice, shared) = voice(0, Source::Sink { ring: consumer }, 1);
  apply(&mut engine, vec![Command::Start(voice)]);
  let out = left(&render(&mut engine, 100));
  assert!(out.iter().all(|s| *s == 0.5));
  assert_eq!(shared.position(), 100);
  // The voice pulls ahead into its converter; ring plus backlog is what
  // was pushed and not heard.
  let backlog = shared.backlog.load(std::sync::atomic::Ordering::Acquire);
  assert_eq!(ring_shared.queued() + backlog, 200);
  assert_eq!(engine.stats().underruns, 0);
  // A clear from the pusher's side: the ring reads empty at once, the
  // frames it dropped (the converter's included) count as consumed, and
  // what is pushed after it plays.
  producer.restart(0);
  assert_eq!(ring_shared.queued(), 0);
  producer.push(&[0.25; 50]);
  let out = left(&render(&mut engine, 100));
  assert!(out[..50].iter().all(|s| *s == 0.25));
  assert!(out[50..].iter().all(|s| *s == 0.0), "an empty sink is silence");
  assert_eq!(shared.position(), 350);
  assert_eq!(engine.stats().underruns, 1, "an outrun sink is counted");
  assert!(!shared.ended(), "a sink never ends on its own");
  // The pusher says nothing more is coming: silence, uncounted, and the
  // sink stays for a later clear and push.
  producer.set_ended();
  render(&mut engine, 100);
  assert_eq!(engine.stats().underruns, 1, "an ended sink is quiet, not starved");
  assert!(!shared.ended());
  producer.restart(0);
  producer.push(&[0.5; 20]);
  let out = left(&render(&mut engine, 20));
  assert!(out.iter().all(|s| *s == 0.5), "pushes after a clear play again");
}

#[test]
fn a_stream_waits_for_its_restart_then_plays_and_ends_with_its_ring() {
  let mut engine = Engine::new(2);
  let (mut producer, consumer) = ring(1000, 1);
  producer.push(&[9.0; 100]);
  // Started for the restart that has not happened: nothing of the stale
  // ring plays, and the wait is not an underrun.
  let generation = consumer.shared().generation() + 1;
  let (mut voice, shared) = voice(0, Source::Stream { ring: consumer, frames: Some(400), generation }, 1);
  voice.looping = true;
  apply(&mut engine, vec![Command::Start(voice)]);
  assert!(render(&mut engine, 10).iter().all(|s| *s == 0.0));
  assert_eq!(engine.stats().underruns, 0);
  producer.restart(390);
  producer.push(&(0..30).map(|i| i as f32).collect::<Vec<f32>>());
  let out = left(&render(&mut engine, 30));
  assert_eq!(out[0], 0.0);
  assert_eq!(out[29], 29.0);
  assert_eq!(shared.position(), (390 + 30) % 400, "content position wraps on the clip length");
  producer.set_ended();
  render(&mut engine, 10);
  assert!(shared.ended(), "an ended ring ends the voice once drained");
}

#[test]
fn a_stream_voice_over_a_shared_ring_can_be_replaced() {
  // The second consumer over the same ring reads what arrives once the
  // first voice is gone (command order guarantees it); what the first
  // voice had pulled ahead goes with it, as a stop discards.
  let (mut producer, consumer) = ring(100, 1);
  producer.push(&[1.0, 2.0, 3.0, 4.0]);
  let shared_ring = Arc::clone(consumer.shared());
  let mut engine = Engine::new(2);
  let (first, _) = voice(0, Source::Stream { ring: consumer, frames: None, generation: 0 }, 1);
  let slot = first.slot;
  apply(&mut engine, vec![Command::Start(first)]);
  assert_eq!(left(&render(&mut engine, 2)), [1.0, 2.0]);
  let second = RingConsumer::over(shared_ring);
  let (second, _) = voice(1, Source::Stream { ring: second, frames: None, generation: 0 }, 1);
  apply(&mut engine, vec![Command::Stop { slot, fade_frames: 0 }, Command::Start(second)]);
  producer.push(&[5.0, 6.0]);
  assert_eq!(left(&render(&mut engine, 2)), [5.0, 6.0]);
  assert_eq!(engine.stats().live_voices, 1);
}

#[test]
fn positions_are_net_of_the_device_queue() {
  // The device holds 30 frames of earlier blocks when a 100-frame block
  // renders: the block itself is what the device asked for now and counts
  // as played, the 30 before it have not yet, so 70 frames of this voice
  // have reached the speaker; the backlog is the rest of what the converter
  // pulled (one feed of 512 source frames). The next block finds the queue
  // drained.
  const FEED: u64 = 512;
  let mut engine = Engine::new(2);
  let (voice, shared) = clip_voice(0, vec![1.0; 1000]);
  apply(&mut engine, vec![Command::Start(voice)]);
  let mut out = vec![0.0; 200];
  engine.render(&mut out, 0, 30);
  assert_eq!(shared.position(), 70);
  assert_eq!(shared.backlog.load(std::sync::atomic::Ordering::Acquire), FEED - 70);
  engine.render(&mut out, 0, 0);
  assert_eq!(shared.position(), 200);
  assert_eq!(shared.backlog.load(std::sync::atomic::Ordering::Acquire), FEED - 200);
}
