// The recorder: one thread owning a VP9 encoder, an Opus encoder and the
// WebM writer, fed plain values over a bounded channel: RGBA frames with
// their capture timestamps and PCM blocks with theirs, on one timeline the
// sources choose (alloy's taps put both on SDL's ticks). Live sources are
// `VideoInput`s and `AudioInput`s the recorder pulls from with a pump
// thread each (`attach`); a producer that must lose nothing (a transcode)
// pushes through a `Feeder` instead. The
// thread anchors media time on the first input, converts frames to I420,
// cuts the audio into Opus frames and muxes; the caller's thread never
// encodes. A pause is two times on the same timeline: inputs stamped inside
// it are dropped and the time between them is taken out of media time, so
// the clip has no gap and no decision depends on the order a message and a
// frame crossed the channel in. A full channel drops
// a video frame (a camera faster than the encoder: the clip keeps real time
// with fewer frames) and never an audio block. A failure mid-clip is kept
// for `stop` and the shared state, and the rest of the inputs are drained
// unencoded, so a feeder or a pump never blocks on a dead recorder.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use super::encode::{open_encoder, rgba_to_i420, EncodedFrame, EncoderConfig, VideoEncoder};
use super::webm::{WebmAudioTrack, WebmSink, WebmVideoTrack, WebmWriter};
use super::{PixelLayout, StreamError};
use crate::audio::{OpusApplication, OpusEncoder};
use crate::stream::{ByteStream, ChunkSender};

// Chunks (one per written block, so about one per frame plus one per audio
// packet) the live sink holds before the recorder thread blocks on it: a
// consumer that stops pulling stalls the recorder within a second, and the
// feeders drop frames meanwhile instead of queueing them.
const LIVE_CHUNKS: usize = 128;

// How often a recorder blocked on a full live channel looks for room, or
// for the sink's close.
const LIVE_WRITE_POLL: std::time::Duration = std::time::Duration::from_millis(10);

/// The live form's sink and its output: a `WebmSink::Stream` the recorder
/// writes the container into, chunk by chunk, the byte stream a consumer
/// pulls those chunks from, and the handle that ends the sink. Dropping the
/// consumer's end fails the recorder's next write (`network`), which ends
/// the recording. `LiveSink::close`, which a stop calls first, makes every
/// later write a discard instead: a recorder blocked on a consumer that
/// stopped pulling is freed, and the stop's own flush has nowhere to go.
pub fn live_sink() -> (WebmSink, ByteStream, LiveSink) {
  let (tx, stream) = crate::stream::channel(LIVE_CHUNKS);
  let closed = Arc::new(AtomicBool::new(false));
  (WebmSink::Stream(Box::new(ChannelWriter { tx, closed: closed.clone() })), stream, LiveSink { closed })
}

/// The live sink's close: after it, the recorder's writes are discarded.
#[derive(Clone)]
pub struct LiveSink {
  closed: Arc<AtomicBool>,
}

impl LiveSink {
  pub fn close(&self) {
    self.closed.store(true, Ordering::Relaxed);
  }
}

struct ChannelWriter {
  tx: ChunkSender,
  closed: Arc<AtomicBool>,
}

impl std::io::Write for ChannelWriter {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }
    let mut chunk = buf.to_vec();
    loop {
      if self.closed.load(Ordering::Relaxed) {
        return Ok(buf.len());
      }
      match self.tx.try_send(chunk) {
        Ok(()) => return Ok(buf.len()),
        Err(crate::stream::TrySend::Full(back)) => {
          chunk = back;
          std::thread::sleep(LIVE_WRITE_POLL);
        }
        Err(crate::stream::TrySend::Closed) => {
          return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the live stream's consumer is gone"));
        }
      }
    }
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

// Inputs queued ahead of the encoder: a few frames of slack for a burst,
// not a backlog (a slow encoder drops frames instead of falling behind).
const CHANNEL_DEPTH: usize = 6;
// The default video bitrate, in bits per pixel per frame: 640x480 at 30 fps
// comes to about 0.9 Mbit/s, 720p30 to 2.8, the quality a recording wants.
const DEFAULT_BITS_PER_PIXEL: f64 = 0.1;
// The default audio bitrate: Opus at 32 kbit/s is transparent for mono
// voice and close to it for music.
pub const DEFAULT_AUDIO_BITRATE_BPS: u32 = 32_000;
// A keyframe at least this often: what a seek lands on, and what a joining
// viewer of the live form waits for.
pub const KEYFRAME_INTERVAL_US: i64 = 2_000_000;
// The nominal frame rate when the app gives none.
pub const DEFAULT_FRAME_RATE: u32 = 30;
const NS_PER_US: u64 = 1000;
const US_PER_S: u64 = 1_000_000;
// How long a pump waits on its input before looking at the stop flag.
const PUMP_WAIT: Duration = Duration::from_millis(100);
// How long the video pump waits at the stop for the frame still in flight:
// a source may deliver one more (a window tap's last readback lands at the
// next frame, a camera may publish once more), so the clip ends on the
// last frame the source produced.
const STOP_DRAIN: Duration = Duration::from_millis(100);

/// The video bitrate for a frame size and rate under the default budget.
pub fn default_video_bitrate(width: u32, height: u32, frame_rate: u32) -> u32 {
  (width as f64 * height as f64 * frame_rate as f64 * DEFAULT_BITS_PER_PIXEL) as u32
}

pub struct AudioConfig {
  pub sample_rate: u32,
  pub channels: u16,
  pub bitrate_bps: u32,
  pub application: OpusApplication,
}

pub struct RecorderConfig {
  pub video: Option<EncoderConfig>,
  pub audio: Option<AudioConfig>,
  pub sink: WebmSink,
}

/// A frame's RGBA8 bytes, however the source holds them: a `Vec<u8>`, or a
/// handle on the source's own buffer (the camera tap's frame, shared with
/// the raster upload), so a frame is not copied to be encoded.
pub type FrameBytes = Box<dyn AsRef<[u8]> + Send>;

/// What a wait on a live input ended with.
pub enum Next<T> {
  Item(T),
  Timeout,
  /// The source closed; nothing will follow.
  Closed,
}

/// A frame of a live video source.
pub struct VideoFrame {
  /// Increases by one per frame the source produced; the pump asks for a
  /// frame past the last it took.
  pub seq: u64,
  pub rgba: FrameBytes,
  pub width: u32,
  pub height: u32,
  /// On the recorder's timeline.
  pub timestamp_ns: u64,
}

/// A block of samples of a live audio source.
pub struct AudioBlock {
  /// Interleaved f32 at the configured rate and channel count.
  pub samples: Vec<f32>,
  /// The first sample's time on the recorder's timeline.
  pub timestamp_ns: u64,
  /// Samples the source lost before this block: the pump fell behind.
  pub dropped: u64,
}

/// A live video source (a camera, the window) the recorder pulls from on a
/// thread of its own.
pub trait VideoInput: Send {
  /// The newest frame numbered past `after`, waiting up to `timeout`.
  fn next(&self, after: u64, timeout: Duration) -> Next<VideoFrame>;
}

/// A live audio source (a microphone) the recorder pulls from on a thread
/// of its own.
pub trait AudioInput: Send {
  /// Everything buffered once at least `min_samples` are, waiting up to
  /// `timeout`.
  fn next(&mut self, min_samples: usize, timeout: Duration) -> Next<AudioBlock>;
}

/// The live sources a recording pulls from (`Recorder::attach`).
#[derive(Default)]
pub struct Inputs {
  pub video: Option<Box<dyn VideoInput>>,
  pub audio: Option<Box<dyn AudioInput>>,
}

/// One input on the recorder's timeline (nanoseconds, any epoch; both
/// tracks on the same one).
pub enum Input {
  Video {
    rgba: FrameBytes,
    width: u32,
    height: u32,
    timestamp_ns: u64,
  },
  /// Interleaved f32 at the configured rate and channel count.
  Audio {
    samples: Vec<f32>,
    timestamp_ns: u64,
  },
}

enum Message {
  Input(Input),
  Pause { at_ns: u64 },
  Resume { at_ns: u64 },
  Stop,
}

/// What the recorder publishes while it runs: the media written so far and
/// the failure that stopped it, if one did.
pub struct RecorderShared {
  duration_us: AtomicI64,
  error: Mutex<Option<StreamError>>,
}

impl RecorderShared {
  pub fn duration_us(&self) -> i64 {
    self.duration_us.load(Ordering::Relaxed)
  }

  pub fn error(&self) -> Option<StreamError> {
    self.error.lock().unwrap_or_else(PoisonError::into_inner).clone()
  }

  fn fail(&self, error: StreamError) {
    let mut slot = self.error.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
      *slot = Some(error);
    }
  }
}

/// A sender a feeder thread pushes inputs through. Cloneable; the recorder
/// stops when told, not when the feeders drop their senders.
#[derive(Clone)]
pub struct Feeder(SyncSender<Message>);

/// What became of a pushed frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Push {
  Queued,
  /// The queue was full (the encoder is behind): the frame is lost.
  Dropped,
  /// The recorder is gone.
  Closed,
}

impl Feeder {
  /// Queue a frame without waiting. The frame's bytes are read on the
  /// recorder's thread and released after the encode.
  pub fn push_video(
    &self,
    rgba: impl AsRef<[u8]> + Send + 'static,
    width: u32,
    height: u32,
    timestamp_ns: u64,
  ) -> Push {
    self.push_frame(Box::new(rgba), width, height, timestamp_ns)
  }

  fn push_frame(&self, rgba: FrameBytes, width: u32, height: u32, timestamp_ns: u64) -> Push {
    match self.0.try_send(Message::Input(Input::Video { rgba, width, height, timestamp_ns })) {
      Ok(()) => Push::Queued,
      Err(TrySendError::Full(_)) => Push::Dropped,
      Err(TrySendError::Disconnected(_)) => Push::Closed,
    }
  }

  /// Queue a frame, waiting for room: for a producer that must lose none
  /// (a transcode from a file); a live source uses `push_video`. Err once
  /// the recorder is gone.
  pub fn send_video(
    &self,
    rgba: impl AsRef<[u8]> + Send + 'static,
    width: u32,
    height: u32,
    timestamp_ns: u64,
  ) -> Result<(), String> {
    let rgba: FrameBytes = Box::new(rgba);
    self
      .0
      .send(Message::Input(Input::Video { rgba, width, height, timestamp_ns }))
      .map_err(|_| "recorder stopped".to_string())
  }

  /// Queue a block of samples, waiting for room; Err once the recorder is
  /// gone.
  pub fn push_audio(&self, samples: Vec<f32>, timestamp_ns: u64) -> Result<(), String> {
    self.0.send(Message::Input(Input::Audio { samples, timestamp_ns })).map_err(|_| "recorder stopped".to_string())
  }
}

pub struct Recorder {
  tx: SyncSender<Message>,
  thread: Option<JoinHandle<Result<(), StreamError>>>,
  shared: Arc<RecorderShared>,
  tracks: Opened,
  /// Raised at the stop (or the drop): the pumps end within `PUMP_WAIT`.
  stop_pumps: Arc<AtomicBool>,
  pumps: Vec<JoinHandle<()>>,
}

/// What the recorder thread opened, for `attach` to check its inputs
/// against.
#[derive(Clone, Copy)]
struct Opened {
  video: bool,
  /// Samples in one Opus frame, all channels: the block the audio pump
  /// waits for, so the encoder never waits on a half frame.
  audio_block: Option<usize>,
}

impl Recorder {
  /// Start a recording: the thread opens the encoders and writes the
  /// container header before this returns, so a failure to start is this
  /// call's error (`unsupported` for an encoder, `network` for the sink).
  pub fn open(config: RecorderConfig) -> Result<Recorder, StreamError> {
    if config.video.is_none() && config.audio.is_none() {
      return Err(StreamError::unsupported("nothing to record: no video and no audio source"));
    }
    let (tx, rx) = mpsc::sync_channel(CHANNEL_DEPTH);
    let (ready_tx, ready_rx) = mpsc::channel();
    let shared = Arc::new(RecorderShared { duration_us: AtomicI64::new(0), error: Mutex::new(None) });
    let thread_shared = shared.clone();
    let thread = std::thread::Builder::new()
      .name("sol-record".to_string())
      .spawn(move || run(config, ready_tx, rx, thread_shared))
      .map_err(|e| StreamError::unsupported(format!("spawn the recorder thread: {e}")))?;
    match ready_rx.recv() {
      Ok(Ok(tracks)) => Ok(Recorder {
        tx,
        thread: Some(thread),
        shared,
        tracks,
        stop_pumps: Arc::new(AtomicBool::new(false)),
        pumps: Vec::new(),
      }),
      Ok(Err(e)) => {
        let _ = thread.join();
        Err(e)
      }
      Err(_) => {
        let _ = thread.join();
        Err(StreamError::unsupported("the recorder thread ended before it was ready"))
      }
    }
  }

  pub fn feeder(&self) -> Feeder {
    Feeder(self.tx.clone())
  }

  /// Pull from live sources until the stop, each on a thread of its own:
  /// every frame the video source publishes (one the encoder has no room
  /// for is lost, so the clip keeps real time) and every sample of the
  /// audio source, in blocks of one Opus frame. Err for an input with no
  /// track to take it, or when a thread cannot start.
  pub fn attach(&mut self, inputs: Inputs) -> Result<(), StreamError> {
    if let Some(video) = inputs.video {
      if !self.tracks.video {
        return Err(StreamError::unsupported("a video input on a recording without video"));
      }
      let (feeder, stop) = (self.feeder(), self.stop_pumps.clone());
      self.pumps.push(spawn_pump("sol-record-video", move || pump_video(video, feeder, stop))?);
    }
    if let Some(audio) = inputs.audio {
      let Some(block) = self.tracks.audio_block else {
        return Err(StreamError::unsupported("an audio input on a recording without audio"));
      };
      let (feeder, stop) = (self.feeder(), self.stop_pumps.clone());
      self.pumps.push(spawn_pump("sol-record-audio", move || pump_audio(audio, block, feeder, stop))?);
    }
    Ok(())
  }

  pub fn shared(&self) -> Arc<RecorderShared> {
    self.shared.clone()
  }

  /// Drop every input stamped from `at_ns` (on the feeders' timeline)
  /// until the resume.
  pub fn pause(&self, at_ns: u64) {
    let _ = self.tx.send(Message::Pause { at_ns });
  }

  /// Take inputs stamped from `at_ns` on again; media time continues where
  /// the pause began.
  pub fn resume(&self, at_ns: u64) {
    let _ = self.tx.send(Message::Resume { at_ns });
  }

  /// End the recording: stop the pumps (the video pump takes the frame in
  /// flight first), flush the encoders, finish the container, join the
  /// threads. Returns the failure that stopped it, if one did.
  pub fn stop(mut self) -> Result<(), StreamError> {
    self.stop_pumps.store(true, Ordering::Relaxed);
    for pump in std::mem::take(&mut self.pumps) {
      let _ = pump.join();
    }
    let _ = self.tx.send(Message::Stop);
    self.join()
  }

  fn join(&mut self) -> Result<(), StreamError> {
    match self.thread.take() {
      Some(thread) => thread.join().unwrap_or_else(|_| Err(StreamError::decode("the recorder thread panicked"))),
      None => Ok(()),
    }
  }
}

impl Drop for Recorder {
  // A handle dropped without `stop` still ends the threads (the pumps at
  // their next look at the flag), and the recorder finishes the container
  // on its way out; nothing waits here.
  fn drop(&mut self) {
    self.stop_pumps.store(true, Ordering::Relaxed);
    if self.thread.is_some() {
      let _ = self.tx.send(Message::Stop);
    }
  }
}

fn spawn_pump(name: &str, body: impl FnOnce() + Send + 'static) -> Result<JoinHandle<()>, StreamError> {
  std::thread::Builder::new()
    .name(name.to_string())
    .spawn(body)
    .map_err(|e| StreamError::unsupported(format!("spawn {name}: {e}")))
}

/// The video pump's cursor and its counts: frames taken and dropped.
#[derive(Default)]
struct VideoCounts {
  after: u64,
  taken: u64,
  dropped: u64,
}

impl VideoCounts {
  /// Hand a frame to the recorder; false once the recorder is closed.
  fn take(&mut self, frame: VideoFrame, feeder: &Feeder) -> bool {
    self.after = frame.seq;
    match feeder.push_frame(frame.rgba, frame.width, frame.height, frame.timestamp_ns) {
      Push::Queued => self.taken += 1,
      Push::Dropped => self.dropped += 1,
      Push::Closed => return false,
    }
    true
  }
}

/// Every frame the source publishes, newest first, until it closes or the
/// stop, plus the one frame in flight at the stop.
fn pump_video(input: Box<dyn VideoInput>, feeder: Feeder, stop: Arc<AtomicBool>) {
  let mut counts = VideoCounts::default();
  let mut open = true;
  while open && !stop.load(Ordering::Relaxed) {
    match input.next(counts.after, PUMP_WAIT) {
      Next::Item(frame) => open = counts.take(frame, &feeder),
      Next::Timeout => {}
      Next::Closed => open = false,
    }
  }
  if open {
    if let Next::Item(frame) = input.next(counts.after, STOP_DRAIN) {
      counts.take(frame, &feeder);
    }
  }
  if counts.dropped > 0 {
    log::warn!(
      "[video] recording dropped {} of {} frames (the encoder fell behind)",
      counts.dropped,
      counts.taken + counts.dropped
    );
  }
}

/// Every sample of the source, in blocks of at least `block` samples,
/// until it closes or the stop.
fn pump_audio(mut input: Box<dyn AudioInput>, block: usize, feeder: Feeder, stop: Arc<AtomicBool>) {
  while !stop.load(Ordering::Relaxed) {
    match input.next(block, PUMP_WAIT) {
      Next::Item(audio) => {
        if audio.dropped > 0 {
          log::warn!("[video] recording lost {} microphone samples (the recorder fell behind)", audio.dropped);
        }
        if feeder.push_audio(audio.samples, audio.timestamp_ns).is_err() {
          break;
        }
      }
      Next::Timeout => {}
      Next::Closed => break,
    }
  }
}

/// Media time over the feeders' timeline: anchored on the first input,
/// less the time spent paused.
struct Timeline {
  t0_ns: Option<u64>,
  excluded_ns: u64,
  /// The pause in progress, from when.
  paused_at_ns: Option<u64>,
  /// Where the last pause ended: an input stamped before it was captured
  /// inside the pause, however late it arrives.
  resumed_at_ns: u64,
}

impl Timeline {
  fn new() -> Timeline {
    Timeline { t0_ns: None, excluded_ns: 0, paused_at_ns: None, resumed_at_ns: 0 }
  }

  fn pause(&mut self, at_ns: u64) {
    if self.paused_at_ns.is_none() {
      self.paused_at_ns = Some(at_ns);
    }
  }

  fn resume(&mut self, at_ns: u64) {
    if let Some(paused_at) = self.paused_at_ns.take() {
      self.excluded_ns += at_ns.saturating_sub(paused_at);
      self.resumed_at_ns = at_ns;
    }
  }

  /// The media time of an input stamped `timestamp_ns`, or None when it
  /// falls in a pause (the input is dropped).
  fn media_us(&mut self, timestamp_ns: u64) -> Option<i64> {
    if self.paused_at_ns.is_some_and(|at| timestamp_ns >= at) || timestamp_ns < self.resumed_at_ns {
      return None;
    }
    let t0 = *self.t0_ns.get_or_insert(timestamp_ns);
    Some((timestamp_ns.saturating_sub(t0).saturating_sub(self.excluded_ns) / NS_PER_US) as i64)
  }
}

struct Tracks {
  video: Option<VideoTrack>,
  audio: Option<AudioTrack>,
  writer: WebmWriter,
}

struct VideoTrack {
  encoder: Box<dyn VideoEncoder>,
  width: u32,
  height: u32,
  frame_us: i64,
  i420: Vec<u8>,
  last_pts_us: Option<i64>,
  /// Frames encoded and the time the conversion and the encode took, for
  /// the line logged at the end: the cost per frame on the recorder's
  /// thread, the number a device's ceiling is read from.
  frames: u64,
  encode: std::time::Duration,
}

struct AudioTrack {
  encoder: OpusEncoder,
  sample_rate: u32,
  channels: u16,
  pending: Vec<f32>,
  /// Media time of the first sample, once there is one; packets are timed
  /// by the samples emitted since.
  start_us: Option<i64>,
  emitted: u64,
}

fn run(
  config: RecorderConfig,
  ready: mpsc::Sender<Result<Opened, StreamError>>,
  rx: Receiver<Message>,
  shared: Arc<RecorderShared>,
) -> Result<(), StreamError> {
  let mut tracks = match open_tracks(config) {
    Ok(tracks) => {
      let _ = ready.send(Ok(Opened {
        video: tracks.video.is_some(),
        audio_block: tracks.audio.as_ref().map(|a| a.encoder.frame_samples() * a.channels as usize),
      }));
      tracks
    }
    Err(e) => {
      let _ = ready.send(Err(e.clone()));
      return Err(e);
    }
  };
  let mut timeline = Timeline::new();
  let mut failed = false;
  loop {
    let message = match rx.recv() {
      Ok(message) => message,
      Err(_) => break,
    };
    match message {
      Message::Stop => break,
      Message::Pause { at_ns } => timeline.pause(at_ns),
      Message::Resume { at_ns } => timeline.resume(at_ns),
      Message::Input(input) => {
        if failed {
          continue;
        }
        let timestamp_ns = match &input {
          Input::Video { timestamp_ns, .. } | Input::Audio { timestamp_ns, .. } => *timestamp_ns,
        };
        let Some(media_us) = timeline.media_us(timestamp_ns) else { continue };
        if let Err(e) = tracks.take(input, media_us) {
          shared.fail(e);
          failed = true;
          continue;
        }
        shared.duration_us.store(tracks.writer.duration_us(), Ordering::Relaxed);
      }
    }
  }
  let finished = if failed {
    Err(shared.error().unwrap_or_else(|| StreamError::decode("recording failed")))
  } else {
    tracks.finish()
  };
  if let Err(e) = &finished {
    shared.fail(e.clone());
  }
  finished
}

fn open_tracks(config: RecorderConfig) -> Result<Tracks, StreamError> {
  let video = config
    .video
    .as_ref()
    .map(|cfg| {
      let encoder = open_encoder(cfg).map_err(StreamError::unsupported)?;
      Ok::<_, StreamError>(VideoTrack {
        encoder,
        width: cfg.width,
        height: cfg.height,
        frame_us: (US_PER_S / cfg.frame_rate.max(1) as u64) as i64,
        i420: Vec::new(),
        last_pts_us: None,
        frames: 0,
        encode: std::time::Duration::ZERO,
      })
    })
    .transpose()?;
  let audio = config
    .audio
    .as_ref()
    .map(|cfg| {
      let encoder = OpusEncoder::new(cfg.sample_rate, cfg.channels, cfg.bitrate_bps, cfg.application)
        .map_err(StreamError::unsupported)?;
      Ok::<_, StreamError>(AudioTrack {
        encoder,
        sample_rate: cfg.sample_rate,
        channels: cfg.channels,
        pending: Vec::new(),
        start_us: None,
        emitted: 0,
      })
    })
    .transpose()?;
  let video_track = video.as_ref().map(|v| WebmVideoTrack { width: v.width, height: v.height });
  let audio_track = audio.as_ref().map(|a| WebmAudioTrack {
    channels: a.channels,
    input_sample_rate: a.sample_rate,
    pre_skip: a.encoder.pre_skip(),
  });
  let writer = WebmWriter::new(config.sink, video_track, audio_track).map_err(StreamError::network)?;
  Ok(Tracks { video, audio, writer })
}

impl Tracks {
  fn take(&mut self, input: Input, media_us: i64) -> Result<(), StreamError> {
    match input {
      Input::Video { rgba, width, height, .. } => {
        let Some(video) = self.video.as_mut() else { return Ok(()) };
        if (width, height) != (video.width, video.height) {
          return Err(StreamError::decode(format!(
            "frame of {width}x{height} in a {}x{} recording",
            video.width, video.height
          )));
        }
        // A frame stamped no later than the last (a source without
        // timestamps stamps 0) takes the nominal step, so the clip plays
        // at its nominal rate rather than stacking frames on one time.
        let pts_us = match video.last_pts_us {
          Some(last) if media_us <= last => last + video.frame_us,
          _ => media_us,
        };
        video.last_pts_us = Some(pts_us);
        let started = std::time::Instant::now();
        rgba_to_i420((*rgba).as_ref(), width, height, &mut video.i420).map_err(StreamError::decode)?;
        debug_assert_eq!(video.i420.len(), PixelLayout::I420.frame_size(width, height));
        let frames = video.encoder.encode(&video.i420, pts_us, false).map_err(StreamError::decode)?;
        video.encode += started.elapsed();
        video.frames += 1;
        write_frames(&mut self.writer, &frames)
      }
      Input::Audio { samples, .. } => {
        let Some(audio) = self.audio.as_mut() else { return Ok(()) };
        if audio.start_us.is_none() {
          audio.start_us = Some(media_us);
        }
        audio.pending.extend_from_slice(&samples);
        audio.drain(&mut self.writer, false)
      }
    }
  }

  fn finish(mut self) -> Result<(), StreamError> {
    if let Some(video) = self.video.as_mut() {
      let frames = video.encoder.flush().map_err(StreamError::decode)?;
      write_frames(&mut self.writer, &frames)?;
      let per_frame_ms = video.encode.as_secs_f64() * 1000.0 / video.frames.max(1) as f64;
      log::info!(
        "[forge::video] recording finished: {} frames of {}x{} at {per_frame_ms:.1} ms each (convert + encode), {:.1} s",
        video.frames,
        video.width,
        video.height,
        self.writer.duration_us() as f64 / 1_000_000.0
      );
    }
    if let Some(audio) = self.audio.as_mut() {
      audio.drain(&mut self.writer, true)?;
    }
    self.writer.finish().map_err(StreamError::network)
  }
}

impl AudioTrack {
  /// Encode every whole frame pending; at the end, the last partial one
  /// padded with silence so no sample is lost.
  fn drain(&mut self, writer: &mut WebmWriter, last: bool) -> Result<(), StreamError> {
    let frame = self.encoder.frame_samples() * self.channels as usize;
    if last && !self.pending.is_empty() {
      let short = self.pending.len() % frame;
      if short != 0 {
        self.pending.resize(self.pending.len() + frame - short, 0.0);
      }
    }
    let start_us = self.start_us.unwrap_or(0);
    let mut at = 0;
    while self.pending.len() - at >= frame {
      let packet = self.encoder.encode(&self.pending[at..at + frame]).map_err(StreamError::decode)?;
      let pts_us = start_us + (self.emitted * US_PER_S / self.sample_rate as u64) as i64;
      writer.write_audio(pts_us, &packet).map_err(StreamError::network)?;
      self.emitted += self.encoder.frame_samples() as u64;
      at += frame;
    }
    self.pending.drain(..at);
    Ok(())
  }
}

fn write_frames(writer: &mut WebmWriter, frames: &[EncodedFrame]) -> Result<(), StreamError> {
  for frame in frames {
    writer.write_video(frame).map_err(StreamError::network)?;
  }
  Ok(())
}
