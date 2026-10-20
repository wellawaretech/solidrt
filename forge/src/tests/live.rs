//! The live form end to end: a recorder writing into its live sink, the
//! stream read as a media source by a reader under the live policy, and
//! the texture player over it. Frames reach the sink as they are made and
//! the player ends when the recording stops; a consumer that does not keep
//! up (a paused player, a stall's backlog) sees the reader cut its backlog
//! to the latency, through the keyframe before it; a source paced at the
//! latency floor plays without a buffering episode.

use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::source::from_stream;
use crate::video::reader::{Live, Next, Reader, LIVE_CATCH_UP_MARGIN_US};
use crate::video::transport::{Clock, FrameSink, LIVE_MIN_LATENCY_US};
use crate::video::{
  live_sink, open_texture, EncodeMode, EncoderConfig, PixelLayout, Recorder, RecorderConfig, YuvFrame,
};

// The chunk size a transport might deliver the stream in, unrelated to the
// container's element boundaries (a QUIC read, say).
const TRANSPORT_CHUNK: usize = 27_001;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const FRAME_RATE: u32 = 30;
const FRAME_NS: u64 = 1_000_000_000 / FRAME_RATE as u64;
const BITRATE_BPS: u32 = 400_000;
const KEYFRAME_INTERVAL_US: i64 = 2_000_000;
// The feeders' timeline starts somewhere, not at 0.
const EPOCH_NS: u64 = 5_000_000_000;
// The latency the live player is opened with.
const LATENCY_US: i64 = 300_000;
// Frames the round trip records: well past the player's start.
const ROUND_TRIP_FRAMES: usize = 90;
// Frames the catch-up test feeds a paused player: past the keyframes at
// 2 s and 4 s, the latency behind the last of them reaching back before
// the second, so the cut resumes through the first.
const CATCH_UP_FRAMES: usize = 121;
const SECOND_KEYFRAME_INDEX: usize = 60;
// Frames the same test feeds on top: past the keyframe at 6 s, the target
// reaching back before it, so the moved cut resumes through the one at
// 4 s.
const MOVED_FRAMES: usize = 181;
const THIRD_KEYFRAME_INDEX: usize = 120;
// The floor tests: frames fed at the frame interval (5 s), and the jitter
// pattern laid over it, in milliseconds per frame (what a Wi-Fi hop does
// to arrival times; its swing stays inside the floor's hysteresis).
const PACED_FRAMES: usize = 150;
const JITTER_MS: [i64; 8] = [-25, 25, 0, 30, -30, 10, -10, 0];
// The stall test: paced frames up to here, then the source stalls for
// STALL_FRAMES frames' worth of time and delivers them at once, then paces
// on to PACED_FRAMES.
const STALL_AT: usize = 66;
const STALL_FRAMES: usize = 30;
// Frames after the burst the recovery may take (the cut and the decode up
// to its target) before every paced frame is shown again.
const SETTLE_FRAMES: usize = 8;
// How often a paused worker looks at the reader (worker.rs IDLE_POLL),
// and the wait that lets it do so twice.
const IDLE_POLL: Duration = Duration::from_millis(250);
// How long to wait for the worker to reach a state.
const PATIENCE: Duration = Duration::from_secs(20);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  m.lock().unwrap_or_else(PoisonError::into_inner)
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

/// A byte stream re-cut into chunks of a fixed size, whatever the chunks
/// it is fed in: the boundaries a transport imposes on a container stream.
struct Rechunk {
  inner: crate::stream::ByteStream,
  pending: Vec<u8>,
  size: usize,
  ended: bool,
}

impl futures_core::Stream for Rechunk {
  type Item = Result<bytes::Bytes, std::io::Error>;

  fn poll_next(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Self::Item>> {
    use std::task::Poll;
    loop {
      if self.pending.len() >= self.size {
        let size = self.size;
        let out = self.pending.drain(..size).collect::<Vec<u8>>();
        return Poll::Ready(Some(Ok(bytes::Bytes::from(out))));
      }
      if self.ended {
        if self.pending.is_empty() {
          return Poll::Ready(None);
        }
        let out = std::mem::take(&mut self.pending);
        return Poll::Ready(Some(Ok(bytes::Bytes::from(out))));
      }
      match self.inner.as_mut().poll_next(cx) {
        Poll::Pending => {
          // A transport delivers what it has: hand out a partial chunk
          // rather than wait for a full one, so a live stream never stalls
          // behind the chunk size.
          if !self.pending.is_empty() {
            let out = std::mem::take(&mut self.pending);
            return Poll::Ready(Some(Ok(bytes::Bytes::from(out))));
          }
          return Poll::Pending;
        }
        Poll::Ready(Some(Ok(chunk))) => self.pending.extend_from_slice(&chunk),
        Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
        Poll::Ready(None) => self.ended = true,
      }
    }
  }
}

/// The content time of the frame at `index`, as the container carries it:
/// the recorder's stamp rounded to the millisecond timecode.
fn pts_us(index: usize) -> i64 {
  ((index as u64 * FRAME_NS + 500_000) / 1_000_000 * 1000) as i64
}

/// A cut's resume position for a stream whose newest frame is `newest_us`:
/// the latency behind it, less up to the margin the lead may grow before
/// the next cut moves it on.
fn cut_range(newest_us: i64, latency_us: i64) -> std::ops::Range<i64> {
  (newest_us - latency_us - LIVE_CATCH_UP_MARGIN_US)..(newest_us - latency_us + FRAME_NS as i64 / 1000)
}

/// A live recorder and the reader over its output, under the live policy.
fn open_live() -> (Recorder, Reader) {
  open_live_with(false, LATENCY_US)
}

/// `open_live`, the stream re-cut at `TRANSPORT_CHUNK` boundaries when
/// `rechunk`, the reader at `latency_us`.
fn open_live_with(rechunk: bool, latency_us: i64) -> (Recorder, Reader) {
  let (sink, stream, _live) = live_sink();
  let stream: crate::stream::ByteStream = if rechunk {
    Box::pin(Rechunk { inner: stream, pending: Vec::new(), size: TRANSPORT_CHUNK, ended: false })
  } else {
    stream
  };
  let recorder = Recorder::open(RecorderConfig {
    video: Some(EncoderConfig {
      width: WIDTH,
      height: HEIGHT,
      frame_rate: FRAME_RATE,
      bitrate_bps: BITRATE_BPS,
      keyframe_interval_us: KEYFRAME_INTERVAL_US,
      mode: EncodeMode::Live,
    }),
    audio: None,
    sink,
  })
  .expect("open recorder");
  let (reader, opened) = Reader::open_with(Box::new(move || from_stream(stream)), Some(Live::with_latency(latency_us)));
  // The header is written at the open, the first keyframe with the first
  // frame: the reader answers once that frame is in.
  let feeder = recorder.feeder();
  feeder.send_video(frame(0), WIDTH, HEIGHT, EPOCH_NS).expect("first frame");
  let info = opened.blocking_recv().expect("the reader answers the open").expect("open");
  assert!(!info.seekable);
  assert_eq!(info.duration_us, None);
  (recorder, reader)
}

/// Feed frames `from..to`, at the frame interval when `paced` (a source in
/// real time), else as fast as the recorder takes them (a burst).
fn feed(recorder: &Recorder, from: usize, to: usize, paced: bool) {
  let feeder = recorder.feeder();
  for index in from..to {
    if paced {
      thread::sleep(Duration::from_nanos(FRAME_NS));
    }
    feeder.send_video(frame(index), WIDTH, HEIGHT, EPOCH_NS + index as u64 * FRAME_NS).expect("frame");
  }
}

/// Feed frames `from..to` in real time with the arrival jitter of
/// `JITTER_MS` laid over the frame interval: the content times stay on
/// the grid, the delivery times do not.
fn feed_jittered(recorder: &Recorder, from: usize, to: usize) {
  let feeder = recorder.feeder();
  let start = Instant::now();
  for index in from..to {
    let due_ms = (index - from) as i64 * FRAME_NS as i64 / 1_000_000 + JITTER_MS[index % JITTER_MS.len()];
    let due = start + Duration::from_millis(due_ms.max(0) as u64);
    thread::sleep(due.saturating_duration_since(Instant::now()));
    feeder.send_video(frame(index), WIDTH, HEIGHT, EPOCH_NS + index as u64 * FRAME_NS).expect("frame");
  }
}

/// Count the player's buffering episodes (a rise of `buffering`) on a
/// thread, until the flag is dropped.
struct EpisodeCounter {
  stop: Arc<std::sync::atomic::AtomicBool>,
  thread: Option<thread::JoinHandle<usize>>,
}

impl EpisodeCounter {
  fn watch(player: &crate::video::Player) -> EpisodeCounter {
    let shared = player.shared();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopping = stop.clone();
    let thread = thread::spawn(move || {
      let mut episodes = 0;
      let mut was = false;
      while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
        let now = shared.buffering();
        if now && !was {
          episodes += 1;
        }
        was = now;
        thread::sleep(Duration::from_millis(1));
      }
      episodes
    });
    EpisodeCounter { stop, thread: Some(thread) }
  }

  fn episodes(mut self) -> usize {
    self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    self.thread.take().expect("the counter thread").join().expect("the counter thread ends")
  }
}

#[derive(Default)]
struct SinkLog {
  pushed: Vec<i64>,
  ended: usize,
}

struct StubSink(Arc<Mutex<SinkLog>>);

impl FrameSink for StubSink {
  fn push(&mut self, frame: YuvFrame, _due_ns: i64) {
    assert_eq!((frame.width, frame.height, frame.layout), (WIDTH, HEIGHT, PixelLayout::I420));
    lock(&self.0).pushed.push(frame.pts_us);
  }
  fn shown_pts_us(&self) -> Option<i64> {
    lock(&self.0).pushed.last().copied()
  }
  fn set_playing(&mut self, _playing: bool) {}
  fn period_ns(&self) -> Option<i64> {
    None
  }
  fn end(&mut self) {
    lock(&self.0).ended += 1;
  }
  fn flush(&mut self) {}
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
  let deadline = Instant::now() + PATIENCE;
  while !done() {
    assert!(Instant::now() < deadline, "timed out waiting for {what}");
    thread::sleep(Duration::from_millis(1));
  }
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_live_recording_plays_as_it_is_made_and_ends_with_the_stop() {
  let (recorder, reader) = open_live();
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let at = Arc::new(AtomicI64::new(0));
  let clock = Clock { now_ns: Box::new(move || at.load(std::sync::atomic::Ordering::Relaxed)), stepped: true };
  let player = open_texture(reader, Box::new(StubSink(log.clone())), clock, None).expect("open texture player");
  player.play();
  // A source in real time: playback starts once the lead reached the live
  // resume buffer, and from there every frame made is played as it comes,
  // the lead never growing to a catch-up.
  feed(&recorder, 1, ROUND_TRIP_FRAMES, true);
  wait_until("frames to reach the sink", || lock(&log).pushed.len() >= ROUND_TRIP_FRAMES / 2);
  assert!(!player.finished());
  recorder.stop().expect("stop");
  wait_until("the stream to end", || player.finished());
  let pushed = lock(&log).pushed.clone();
  assert_eq!(pushed.len(), ROUND_TRIP_FRAMES, "every frame made was played");
  assert_eq!(pushed[0], 0, "playback starts at the first keyframe");
  assert!(pushed.windows(2).all(|w| w[1] > w[0]), "frames in order");
  assert_eq!(lock(&log).ended, 1);
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_stream_cut_at_transport_boundaries_decodes_every_frame() {
  let (recorder, reader) = open_live_with(true, LATENCY_US);
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let at = Arc::new(AtomicI64::new(0));
  let clock = Clock { now_ns: Box::new(move || at.load(std::sync::atomic::Ordering::Relaxed)), stepped: true };
  let player = open_texture(reader, Box::new(StubSink(log.clone())), clock, None).expect("open texture player");
  player.play();
  feed(&recorder, 1, ROUND_TRIP_FRAMES, true);
  recorder.stop().expect("stop");
  wait_until("the stream to end", || player.finished());
  let pushed = lock(&log).pushed.clone();
  assert_eq!(pushed.len(), ROUND_TRIP_FRAMES, "every frame decoded across the chunk boundaries");
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_paused_player_has_its_backlog_cut_to_the_latency_through_the_keyframe_before_it() {
  let (recorder, reader) = open_live();
  let live = reader.live().expect("a live reader");
  // A burst: nobody releases frames, the lead grows past the latency and
  // the margin, and the reader cuts: the epoch resumes at the latency
  // behind the newest frame, with the queue starting at the newest
  // keyframe before the target (2 s), not at the one after it (4 s),
  // which would resume inside the latency.
  feed(&recorder, 1, CATCH_UP_FRAMES, false);
  let newest_us = pts_us(CATCH_UP_FRAMES - 1);
  wait_until("the burst to arrive", || reader.status().newest_us == newest_us);
  let status = reader.status();
  let resume_us = status.resume_us.expect("a resume position");
  assert_eq!(status.epoch, 1, "one epoch for the whole burst: nothing was released in between");
  assert!(cut_range(newest_us, live.latency_us).contains(&resume_us), "resumes at the latency: {resume_us} us");
  assert!(status.lead_us <= live.latency_us + LIVE_CATCH_UP_MARGIN_US, "the lead is back inside the latency");
  assert_eq!(status.lead_us, newest_us - resume_us, "the lead is measured from the target");
  match reader.video_ready() {
    Next::Packet(()) => {}
    _ => panic!("a packet is queued"),
  }
  // The live edge moves on while nothing has been released: the target
  // moves within the epoch, through the next keyframe (4 s) now that it
  // is the newest before the target.
  feed(&recorder, CATCH_UP_FRAMES, MOVED_FRAMES, false);
  let newest_us = pts_us(MOVED_FRAMES - 1);
  wait_until("the second burst to arrive", || reader.status().newest_us == newest_us);
  let status = reader.status();
  let resume_us = status.resume_us.expect("a resume position");
  assert_eq!(status.epoch, 1, "the target moved within the epoch");
  assert!(cut_range(newest_us, live.latency_us).contains(&resume_us), "the target moved: {resume_us} us");
  match reader.next_video() {
    Next::Packet(au) => {
      assert!(au.sync, "the queue starts at a keyframe");
      assert_eq!(au.pts_us, pts_us(THIRD_KEYFRAME_INDEX), "the newest keyframe before the moved target");
    }
    _ => panic!("a packet is queued"),
  }
  recorder.stop().expect("stop");
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_release_from_before_the_cut_does_not_move_the_head_back() {
  let (recorder, reader) = open_live();
  let live = reader.live().expect("a live reader");
  feed(&recorder, 1, CATCH_UP_FRAMES, false);
  let newest_us = pts_us(CATCH_UP_FRAMES - 1);
  wait_until("the burst to arrive", || reader.status().newest_us == newest_us);
  let status = reader.status();
  let resume_us = status.resume_us.expect("a resume position");
  // The player had a frame from before the cut in hand and releases it
  // now: the head stays at the target (the lead does not grow back past
  // the margin, which would cut again), and the target still moves within
  // the epoch when the edge moves on.
  reader.released(resume_us - FRAME_NS as i64 / 1000);
  let after = reader.status();
  assert_eq!(after.lead_us, status.lead_us, "a stale release leaves the head at the target");
  assert_eq!(after.epoch, 1);
  feed(&recorder, CATCH_UP_FRAMES, MOVED_FRAMES, false);
  wait_until("the second burst to arrive", || reader.status().newest_us == pts_us(MOVED_FRAMES - 1));
  let moved = reader.status();
  assert_eq!(moved.epoch, 1, "the target moved within the epoch");
  assert!(cut_range(pts_us(MOVED_FRAMES - 1), live.latency_us).contains(&moved.resume_us.expect("a resume position")));
  recorder.stop().expect("stop");
}

#[cfg(not(target_os = "android"))]
#[test]
fn play_after_a_pause_resumes_at_the_moved_target() {
  let (recorder, reader) = open_live();
  let live = reader.live().expect("a live reader");
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let at = Arc::new(AtomicI64::new(0));
  let clock = Clock { now_ns: Box::new(move || at.load(std::sync::atomic::Ordering::Relaxed)), stepped: true };
  let player = open_texture(reader, Box::new(StubSink(log.clone())), clock, None).expect("open texture player");
  // Paused through two bursts: the worker takes the cut's epoch after the
  // first (it looks at the reader while paused) and the moved target
  // after the second; play then decodes from the keyframe at 4 s and shows
  // the frame at the moved target first.
  feed(&recorder, 1, CATCH_UP_FRAMES, false);
  thread::sleep(IDLE_POLL * 2);
  feed(&recorder, CATCH_UP_FRAMES, MOVED_FRAMES, false);
  thread::sleep(IDLE_POLL * 2);
  assert!(!player.buffering(), "a paused player is not buffering");
  player.play();
  wait_until("the first frame to show", || !lock(&log).pushed.is_empty());
  let first = lock(&log).pushed[0];
  assert!(cut_range(pts_us(MOVED_FRAMES - 1), live.latency_us).contains(&first), "first frame at {first} us");
  recorder.stop().expect("stop");
  wait_until("the stream to end", || player.finished());
  let pushed = lock(&log).pushed.clone();
  assert!(pushed.windows(2).all(|w| w[1] > w[0]), "frames in order, none shown twice");
  assert_eq!(*pushed.last().expect("frames"), pts_us(MOVED_FRAMES - 1), "played to the end");
  assert!(!pushed.contains(&pts_us(SECOND_KEYFRAME_INDEX)), "the keyframe before the first target was skipped");
  assert!(!pushed.contains(&pts_us(THIRD_KEYFRAME_INDEX)), "the keyframe before the moved target was decoded unshown");
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_source_paced_at_the_floor_plays_without_buffering() {
  let (recorder, reader) = open_live_with(false, LIVE_MIN_LATENCY_US);
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let player =
    open_texture(reader, Box::new(StubSink(log.clone())), Clock::monotonic(), None).expect("open texture player");
  let counter = EpisodeCounter::watch(&player);
  player.play();
  // Frames arrive in real time with jitter inside the floor's hysteresis:
  // the start is the one buffering episode (the lead has to reach the
  // latency), then every frame is shown, none dropped as late and none
  // skipped by a catch-up.
  feed_jittered(&recorder, 1, PACED_FRAMES);
  recorder.stop().expect("stop");
  wait_until("the stream to end", || player.finished());
  let episodes = counter.episodes();
  let pushed = lock(&log).pushed.clone();
  let missing: Vec<usize> = (0..PACED_FRAMES).filter(|&index| !pushed.contains(&pts_us(index))).collect();
  assert!(missing.is_empty(), "frames not shown: {missing:?} ({episodes} buffering episodes)");
  assert!(pushed.windows(2).all(|w| w[1] > w[0]), "frames in order");
  assert!(episodes <= 1, "{episodes} buffering episodes: only the start may be one");
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_stall_is_recovered_at_the_latency_through_the_keyframe_before_it() {
  let (recorder, reader) = open_live_with(false, LIVE_MIN_LATENCY_US);
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let player =
    open_texture(reader, Box::new(StubSink(log.clone())), Clock::monotonic(), None).expect("open texture player");
  player.play();
  feed(&recorder, 1, STALL_AT, true);
  // The source stalls for STALL_FRAMES frames' worth of time, then its
  // backlog arrives at once (the frames keep their content times), then
  // it paces on.
  thread::sleep(Duration::from_nanos(FRAME_NS * STALL_FRAMES as u64));
  feed(&recorder, STALL_AT, STALL_AT + STALL_FRAMES, false);
  feed(&recorder, STALL_AT + STALL_FRAMES, PACED_FRAMES, true);
  recorder.stop().expect("stop");
  wait_until("the stream to end", || player.finished());
  let pushed = lock(&log).pushed.clone();
  assert!(pushed.windows(2).all(|w| w[1] > w[0]), "frames in order, none shown twice");
  // The backlog is skipped, not played out: the first late frame
  // re-anchors the clock and the backlog plays at the frame interval only
  // until the lead passes the latency and the margin, when the player cuts
  // to the latency behind the newest frame. The burst arrives at the
  // encoder's pace here (a few times real time), so a few frames show
  // between cuts and no run of shown backlog frames gets anywhere near
  // the whole backlog; a network burst shows one or two. The cut went
  // through the keyframe at 2 s (the newest before the target), decoded
  // unshown: it is not shown again.
  let backlog: Vec<i64> =
    pushed.iter().copied().filter(|&pts| pts >= pts_us(STALL_AT) && pts < pts_us(STALL_AT + STALL_FRAMES)).collect();
  assert!(backlog.len() < STALL_FRAMES, "the whole backlog played out: {backlog:?}");
  let frame_us = FRAME_NS as i64 / 1000;
  let mut run = 1;
  let mut longest_run = 1;
  for pair in backlog.windows(2) {
    run = if pair[1] - pair[0] <= frame_us + 1000 { run + 1 } else { 1 };
    longest_run = longest_run.max(run);
  }
  assert!(longest_run < STALL_FRAMES / 2, "a run of {longest_run} backlog frames played out: {backlog:?}");
  assert_eq!(pushed.iter().filter(|&&pts| pts == pts_us(SECOND_KEYFRAME_INDEX)).count(), 1);
  // Everything paced after the burst is shown: the player is back at the
  // latency, with nothing late.
  for index in STALL_AT + STALL_FRAMES + SETTLE_FRAMES..PACED_FRAMES {
    assert!(pushed.contains(&pts_us(index)), "frame {index} after the stall was not shown");
  }
}

/// A live stream as a Pixel 7's hardware encoder made it, captured on the
/// receiving laptop (the first second: 60 frames, one keyframe). Its
/// frames carry the encoder's output padding after the last tile, zeros
/// mostly, a run of 0xff on some: what libvpx took for a second frame with
/// an invalid marker until the decoder stated each frame's length.
fn pixel_live_path() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/pixel_live.webm").to_string()
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_pixel_live_stream_decodes_frame_by_frame() {
  use crate::source::Facts;
  use crate::video::{Demuxer, Packet, WebmDemuxer};
  let bytes = std::fs::read(pixel_live_path()).expect("the fixture");
  let mut demux = WebmDemuxer::open(Box::new(std::io::Cursor::new(bytes)), Facts { len: None, seekable: false })
    .expect("open the stream");
  let info = demux.info().clone();
  let mut decoder = crate::video::Vp9Decoder::new(info.width, info.height).expect("decoder");
  let mut frames = 0;
  let mut failures = Vec::new();
  while let Ok(Some(packet)) = demux.next_packet() {
    if let Packet::Video(au) = packet {
      let head: Vec<String> = au.data.iter().take(4).map(|b| format!("{b:02x}")).collect();
      match crate::video::VideoDecoder::decode(&mut decoder, &au) {
        Ok(_) => frames += 1,
        Err(e) => failures.push(format!("{} us ({} bytes, {}): {e}", au.pts_us, au.data.len(), head.join(" "))),
      }
    }
  }
  assert!(failures.is_empty(), "{frames} frames decoded, failures:\n{}", failures.join("\n"));
  assert_eq!(frames, 60, "every frame of the first second");
}
