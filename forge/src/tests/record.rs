// The recorder end to end: synthetic frames and samples with timestamps in
// through its feeder, a WebM file out, read back through the demuxer: the
// frame times follow the timestamps, a pause leaves no gap, a source that
// stamps nothing still yields a clip at the nominal rate, and audio keeps
// every sample.

use std::io::Cursor;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::sink::SharedSink;
use crate::audio::OpusApplication;
use crate::source::Facts;
use crate::video::{
  AudioBlock, AudioConfig, AudioInput, Demuxer, EncodeMode, EncoderConfig, ErrorKind, Inputs, Next, Packet, Push,
  Recorder, RecorderConfig, VideoFrame, VideoInput, WebmDemuxer, WebmSink,
};

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const FRAME_RATE: u32 = 30;
const FRAME_NS: u64 = 1_000_000_000 / FRAME_RATE as u64;
const FRAMES: usize = 30;
const BITRATE_BPS: u32 = 400_000;
const KEYFRAME_INTERVAL_US: i64 = 2_000_000;
const RATE: u32 = 16_000;
const AUDIO_BITRATE_BPS: u32 = 32_000;
// Audio arrives in blocks of this many samples (an odd size, so Opus
// frames straddle blocks).
const AUDIO_BLOCK: usize = 500;
const AUDIO_SECONDS: usize = 1;
// The feeders' timeline starts somewhere, not at 0.
const EPOCH_NS: u64 = 5_000_000_000;
// A pause this long must not show in the clip.
const PAUSE_NS: u64 = 3_000_000_000;
const PAUSE_AFTER: usize = 10;
// Rounding to the millisecond tick on each frame.
const PTS_TOLERANCE_US: i64 = 1_000;
// Frames a pumped test publishes: fewer than the recorder's queue holds,
// so none is dropped however slow the encoder runs.
const PUMPED_FRAMES: usize = 5;
// One Opus frame at RATE: 20 ms.
const OPUS_FRAME_SAMPLES: usize = 320;
// A wait long enough that a pump's step always lands inside it.
const WAIT: Duration = Duration::from_secs(5);

fn video_config() -> EncoderConfig {
  EncoderConfig {
    width: WIDTH,
    height: HEIGHT,
    frame_rate: FRAME_RATE,
    bitrate_bps: BITRATE_BPS,
    keyframe_interval_us: KEYFRAME_INTERVAL_US,
    mode: EncodeMode::Recording,
  }
}

fn audio_config() -> AudioConfig {
  AudioConfig { sample_rate: RATE, channels: 1, bitrate_bps: AUDIO_BITRATE_BPS, application: OpusApplication::Audio }
}

fn frame(index: usize) -> Vec<u8> {
  let mut rgba = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
  for y in 0..HEIGHT {
    for x in 0..WIDTH {
      rgba.extend_from_slice(&[((x as usize + index) % 256) as u8, (y * 2) as u8, 128, 255]);
    }
  }
  rgba
}

fn open(sink: &SharedSink, video: bool, audio: bool) -> Recorder {
  Recorder::open(RecorderConfig {
    video: video.then(video_config),
    audio: audio.then(audio_config),
    sink: WebmSink::File(Box::new(sink.clone())),
  })
  .expect("open recorder")
}

fn demux(sink: &SharedSink) -> WebmDemuxer {
  let bytes = sink.bytes();
  let len = bytes.len() as u64;
  WebmDemuxer::open(Box::new(Cursor::new(bytes)), Facts { len: Some(len), seekable: true }).expect("open clip")
}

fn read_all(demux: &mut WebmDemuxer) -> (Vec<i64>, Vec<i64>) {
  let (mut video, mut audio) = (Vec::new(), Vec::new());
  while let Some(packet) = demux.next_packet().expect("read") {
    match packet {
      Packet::Video(au) => video.push(au.pts_us),
      Packet::Audio(packet) => audio.push(packet.pts_us),
    }
  }
  (video, audio)
}

fn assert_spacing(pts: &[i64], count: usize, step_us: i64) {
  assert_eq!(pts.len(), count);
  for (index, &pts) in pts.iter().enumerate() {
    let expected = index as i64 * step_us;
    assert!((pts - expected).abs() <= PTS_TOLERANCE_US, "frame {index} at {pts} us, expected ~{expected}");
  }
}

#[test]
fn frames_and_samples_follow_their_timestamps() {
  let sink = SharedSink::default();
  let recorder = open(&sink, true, true);
  let feeder = recorder.feeder();
  let samples: Vec<f32> = (0..RATE as usize * AUDIO_SECONDS).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
  let mut audio_at = 0;
  for index in 0..FRAMES {
    let ts = EPOCH_NS + index as u64 * FRAME_NS;
    feeder.send_video(frame(index), WIDTH, HEIGHT, ts).expect("frame queued");
    // The audio blocks that fall before this frame's time.
    while audio_at < samples.len() && EPOCH_NS + audio_at as u64 * 1_000_000_000 / RATE as u64 <= ts {
      let end = (audio_at + AUDIO_BLOCK).min(samples.len());
      let block_ts = EPOCH_NS + audio_at as u64 * 1_000_000_000 / RATE as u64;
      feeder.push_audio(samples[audio_at..end].to_vec(), block_ts).expect("audio queued");
      audio_at = end;
    }
  }
  while audio_at < samples.len() {
    let end = (audio_at + AUDIO_BLOCK).min(samples.len());
    let block_ts = EPOCH_NS + audio_at as u64 * 1_000_000_000 / RATE as u64;
    feeder.push_audio(samples[audio_at..end].to_vec(), block_ts).expect("audio queued");
    audio_at = end;
  }
  let shared = recorder.shared();
  recorder.stop().expect("stop");
  assert!(shared.error().is_none());
  let duration = shared.duration_us();
  assert!((duration - 1_000_000).abs() <= 40_000, "duration {duration}");

  let mut demux = demux(&sink);
  assert_eq!((demux.info().width, demux.info().height), (WIDTH, HEIGHT));
  assert!(demux.info().audio.is_some());
  let (video, audio) = read_all(&mut demux);
  assert_spacing(&video, FRAMES, (FRAME_NS / 1000) as i64);
  // 1 s of 16 kHz in 20 ms packets, every sample kept (the last packet
  // padded).
  assert_eq!(audio.len(), RATE as usize * AUDIO_SECONDS / 320);
  assert_spacing(&audio, audio.len(), 20_000);
}

#[test]
fn a_pause_leaves_no_gap() {
  let sink = SharedSink::default();
  let recorder = open(&sink, true, false);
  let feeder = recorder.feeder();
  for index in 0..PAUSE_AFTER {
    feeder.send_video(frame(index), WIDTH, HEIGHT, EPOCH_NS + index as u64 * FRAME_NS).expect("frame queued");
  }
  // The pause starts between two frames and lasts PAUSE_NS; frames
  // stamped inside it are dropped, however late they arrive.
  let paused_at = EPOCH_NS + PAUSE_AFTER as u64 * FRAME_NS - FRAME_NS / 2;
  recorder.pause(paused_at);
  for index in PAUSE_AFTER..PAUSE_AFTER + 5 {
    feeder.send_video(frame(index), WIDTH, HEIGHT, EPOCH_NS + index as u64 * FRAME_NS + PAUSE_NS / 2).expect("queued");
  }
  recorder.resume(paused_at + PAUSE_NS);
  feeder.send_video(frame(0), WIDTH, HEIGHT, paused_at + PAUSE_NS / 3).expect("a late frame from inside the pause");
  for index in PAUSE_AFTER..FRAMES {
    feeder
      .send_video(frame(index), WIDTH, HEIGHT, EPOCH_NS + index as u64 * FRAME_NS + PAUSE_NS)
      .expect("frame queued");
  }
  recorder.stop().expect("stop");
  let (video, _) = read_all(&mut demux(&sink));
  assert_spacing(&video, FRAMES, (FRAME_NS / 1000) as i64);
}

#[test]
fn unstamped_frames_take_the_nominal_step() {
  let sink = SharedSink::default();
  let recorder = open(&sink, true, false);
  let feeder = recorder.feeder();
  for index in 0..FRAMES {
    feeder.send_video(frame(index), WIDTH, HEIGHT, 0).expect("frame queued");
  }
  recorder.stop().expect("stop");
  let (video, _) = read_all(&mut demux(&sink));
  assert_spacing(&video, FRAMES, (FRAME_NS / 1000) as i64);
}

#[test]
fn a_wrong_frame_size_fails_the_recording_and_stop_reports_it() {
  let sink = SharedSink::default();
  let recorder = open(&sink, true, false);
  let feeder = recorder.feeder();
  feeder.send_video(frame(0), WIDTH, HEIGHT, EPOCH_NS).expect("frame queued");
  feeder.send_video(vec![0; 16], 2, 2, EPOCH_NS + FRAME_NS).expect("frame queued");
  let shared = recorder.shared();
  let err = recorder.stop().expect_err("the bad frame fails the recording");
  assert_eq!(err.kind, ErrorKind::Decode);
  assert!(err.message.contains("2x2"), "{}", err.message);
  assert_eq!(shared.error().map(|e| e.kind), Some(ErrorKind::Decode));
}

#[test]
fn a_burst_past_the_queue_drops_frames_and_keeps_the_clip() {
  let sink = SharedSink::default();
  let recorder = open(&sink, true, false);
  let feeder = recorder.feeder();
  let outcomes: Vec<Push> = (0..FRAMES)
    .map(|index| feeder.push_video(frame(index), WIDTH, HEIGHT, EPOCH_NS + index as u64 * FRAME_NS))
    .collect();
  assert_eq!(outcomes[0], Push::Queued);
  assert!(!outcomes.contains(&Push::Closed));
  recorder.stop().expect("stop");
  let (video, _) = read_all(&mut demux(&sink));
  let queued = outcomes.iter().filter(|&&p| p == Push::Queued).count();
  assert_eq!(video.len(), queued, "every queued frame is in the clip, no dropped one");
  assert!(video.windows(2).all(|w| w[1] > w[0]), "timestamps stay in order across drops");
}

#[test]
fn open_refuses_what_it_cannot_record() {
  let sink = SharedSink::default();
  let err = Recorder::open(RecorderConfig { video: None, audio: None, sink: WebmSink::File(Box::new(sink.clone())) })
    .err()
    .expect("no sources");
  assert_eq!(err.kind, ErrorKind::Unsupported);
  let err = Recorder::open(RecorderConfig {
    video: None,
    audio: Some(AudioConfig { sample_rate: 44_100, ..audio_config() }),
    sink: WebmSink::File(Box::new(sink)),
  })
  .err()
  .expect("a rate Opus does not take");
  assert_eq!(err.kind, ErrorKind::Unsupported);
  assert!(err.message.contains("48 kHz"), "{}", err.message);
}

/// A live video source the test publishes into, like a tap: frames in
/// order, the cursor the pump last asked past, and a close.
#[derive(Clone, Default)]
struct FakeVideo(Arc<(Mutex<FakeVideoState>, Condvar)>);

#[derive(Default)]
struct FakeVideoState {
  published: u64,
  asked_after: u64,
  closed: bool,
}

impl FakeVideo {
  fn update(&self, f: impl FnOnce(&mut FakeVideoState)) {
    f(&mut self.0 .0.lock().unwrap_or_else(PoisonError::into_inner));
    self.0 .1.notify_all();
  }

  fn publish(&self) {
    self.update(|state| state.published += 1);
  }

  fn close(&self) {
    self.update(|state| state.closed = true);
  }

  /// Block until the pump asks past frame `seq`: it took every frame up to
  /// it.
  fn wait_taken(&self, seq: u64) {
    let deadline = Instant::now() + WAIT;
    let mut state = self.0 .0.lock().unwrap_or_else(PoisonError::into_inner);
    while state.asked_after < seq {
      let now = Instant::now();
      assert!(now < deadline, "the pump never took frame {seq}");
      state = self.0 .1.wait_timeout(state, deadline - now).unwrap_or_else(PoisonError::into_inner).0;
    }
  }
}

impl VideoInput for FakeVideo {
  fn next(&self, after: u64, timeout: Duration) -> Next<VideoFrame> {
    let deadline = Instant::now() + timeout;
    let mut state = self.0 .0.lock().unwrap_or_else(PoisonError::into_inner);
    state.asked_after = state.asked_after.max(after);
    self.0 .1.notify_all();
    loop {
      if state.closed {
        return Next::Closed;
      }
      if state.published > after {
        let index = after as usize;
        return Next::Item(VideoFrame {
          seq: after + 1,
          rgba: Box::new(frame(index)),
          width: WIDTH,
          height: HEIGHT,
          timestamp_ns: EPOCH_NS + index as u64 * FRAME_NS,
        });
      }
      let now = Instant::now();
      if now >= deadline {
        return Next::Timeout;
      }
      state = self.0 .1.wait_timeout(state, deadline - now).unwrap_or_else(PoisonError::into_inner).0;
    }
  }
}

/// A live audio source with `AUDIO_SECONDS` buffered: it hands them out in
/// `AUDIO_BLOCK`s, then closes, reporting the block size the pump asked for.
struct FakeAudio {
  samples: Vec<f32>,
  at: usize,
  asked: mpsc::Sender<usize>,
}

impl AudioInput for FakeAudio {
  fn next(&mut self, min_samples: usize, _timeout: Duration) -> Next<AudioBlock> {
    if self.at == self.samples.len() {
      let _ = self.asked.send(min_samples);
      return Next::Closed;
    }
    let end = (self.at + AUDIO_BLOCK).min(self.samples.len());
    let block = AudioBlock {
      samples: self.samples[self.at..end].to_vec(),
      timestamp_ns: EPOCH_NS + self.at as u64 * 1_000_000_000 / RATE as u64,
      dropped: 0,
    };
    self.at = end;
    Next::Item(block)
  }
}

#[test]
fn an_attached_source_is_pumped_through_the_stop() {
  let sink = SharedSink::default();
  let mut recorder = open(&sink, true, false);
  let source = FakeVideo::default();
  recorder.attach(Inputs { video: Some(Box::new(source.clone())), audio: None }).expect("attach");
  for seq in 1..=PUMPED_FRAMES as u64 {
    source.publish();
    source.wait_taken(seq);
  }
  // One more frame lands as the stop begins: the clip ends on it.
  source.publish();
  recorder.stop().expect("stop");
  let (video, _) = read_all(&mut demux(&sink));
  assert_spacing(&video, PUMPED_FRAMES + 1, (FRAME_NS / 1000) as i64);
}

#[test]
fn a_closed_source_ends_its_pump() {
  let sink = SharedSink::default();
  let mut recorder = open(&sink, true, false);
  let source = FakeVideo::default();
  recorder.attach(Inputs { video: Some(Box::new(source.clone())), audio: None }).expect("attach");
  source.publish();
  source.wait_taken(1);
  source.close();
  source.publish();
  recorder.stop().expect("stop");
  let (video, _) = read_all(&mut demux(&sink));
  assert_eq!(video.len(), 1, "nothing is taken past the close");
}

#[test]
fn an_attached_microphone_is_pumped_in_opus_frames() {
  let sink = SharedSink::default();
  // A video track with one frame: the demuxer reads clips with a keyframe.
  let mut recorder = open(&sink, true, true);
  recorder.feeder().send_video(frame(0), WIDTH, HEIGHT, EPOCH_NS).expect("frame queued");
  let (asked, asked_rx) = mpsc::channel();
  let samples: Vec<f32> = (0..RATE as usize * AUDIO_SECONDS).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
  let source = FakeAudio { samples, at: 0, asked };
  recorder.attach(Inputs { video: None, audio: Some(Box::new(source)) }).expect("attach");
  let min_samples = asked_rx.recv_timeout(WAIT).expect("the pump drained the source");
  assert_eq!(min_samples, OPUS_FRAME_SAMPLES);
  recorder.stop().expect("stop");
  let (_, audio) = read_all(&mut demux(&sink));
  assert_eq!(audio.len(), RATE as usize * AUDIO_SECONDS / OPUS_FRAME_SAMPLES);
  assert_spacing(&audio, audio.len(), 20_000);
}

#[test]
fn attach_refuses_an_input_without_a_track() {
  let sink = SharedSink::default();
  let mut recorder = open(&sink, false, true);
  let err =
    recorder.attach(Inputs { video: Some(Box::new(FakeVideo::default())), audio: None }).expect_err("no video track");
  assert_eq!(err.kind, ErrorKind::Unsupported);
  recorder.stop().expect("stop");
}
