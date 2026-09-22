// The texture player over the real decoder (libvpx on the host) and a stub
// frame sink: frames reach the sink with due times at the content
// interval and the end follows the last, a seek while paused pushes the
// target frame once after flushing the sink, a URL source plays through
// the reader, and a stepped clock pushes every frame without a wait.

use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::source::serve;
use crate::source::Source;
use crate::video::reader::Reader;
use crate::video::transport::{Clock, FrameSink};
use crate::video::{open_texture, PixelLayout, YuvFrame};

/// How long to wait for the worker to reach a state.
const PATIENCE: Duration = Duration::from_secs(20);
/// The fixtures' frame interval (25 fps).
const FRAME_US: i64 = 40_000;

// 4 s of ffmpeg testsrc2 160x120 at 25 fps, no audio, a keyframe every
// second: 100 frames, seekable.
fn kf_path() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_kf.webm").to_string()
}

// 4 s with audio, several GOPs: the streaming fixture.
fn gop_path() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_gop.webm").to_string()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn open_reader(spec: &str) -> Reader {
  let source = Source::parse(spec).expect("a path or URL");
  let (reader, opened) = Reader::open(Box::new(move || source.open("forge-test")));
  opened.blocking_recv().expect("the reader answers the open").expect("open");
  reader
}

/// What the sink saw: every push as (pts, due), the ends, the flushes and
/// the playing transitions, in order of arrival.
#[derive(Default)]
struct SinkLog {
  pushed: Vec<(i64, i64, usize)>,
  ended: usize,
  flushed: usize,
  playing: Vec<bool>,
}

struct StubSink(Arc<Mutex<SinkLog>>);

impl FrameSink for StubSink {
  fn push(&mut self, frame: YuvFrame, due_ns: i64) {
    assert_eq!((frame.width, frame.height, frame.layout), (160, 120, PixelLayout::I420));
    lock(&self.0).pushed.push((frame.pts_us, due_ns, frame.data.len()));
  }
  fn shown_pts_us(&self) -> Option<i64> {
    lock(&self.0).pushed.last().map(|&(pts, _, _)| pts)
  }
  fn set_playing(&mut self, playing: bool) {
    lock(&self.0).playing.push(playing);
  }
  fn period_ns(&self) -> Option<i64> {
    None
  }
  fn end(&mut self) {
    lock(&self.0).ended += 1;
  }
  fn flush(&mut self) {
    lock(&self.0).flushed += 1;
  }
}

fn stepped_clock(at: Arc<AtomicI64>) -> Clock {
  Clock { now_ns: Box::new(move || at.load(std::sync::atomic::Ordering::Relaxed)), stepped: true }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
  let deadline = Instant::now() + PATIENCE;
  while !done() {
    assert!(Instant::now() < deadline, "timed out waiting for {what}");
    thread::sleep(Duration::from_millis(1));
  }
}

fn pushed(log: &Arc<Mutex<SinkLog>>) -> Vec<(i64, i64, usize)> {
  lock(log).pushed.clone()
}

#[cfg(not(target_os = "android"))]
#[test]
fn every_frame_reaches_the_sink_at_the_content_interval_and_the_end_follows() {
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let at = Arc::new(AtomicI64::new(0));
  let player = open_texture(open_reader(&kf_path()), Box::new(StubSink(log.clone())), stepped_clock(at), None)
    .expect("open texture player");
  let started = Instant::now();
  player.play();
  wait_until("the stream to finish", || player.finished());
  // A stepped clock waits for nothing: the whole clip is decoded and
  // pushed far faster than it plays.
  assert!(started.elapsed() < Duration::from_secs(4), "took {:?} for a 4 s clip", started.elapsed());
  let frames = pushed(&log);
  assert_eq!(frames.len(), 100, "4 s at 25 fps");
  let frame_size = PixelLayout::I420.frame_size(160, 120);
  for (k, &(pts, due_ns, len)) in frames.iter().enumerate() {
    assert_eq!(pts, k as i64 * FRAME_US);
    assert_eq!(due_ns, k as i64 * FRAME_US * 1000, "frame {k} due on the anchor at the content interval");
    assert_eq!(len, frame_size);
  }
  let log = lock(&log);
  assert_eq!(log.ended, 1, "the end follows the last frame");
  assert_eq!(log.playing, vec![true, false], "releases started at play and stopped at the end");
  assert_eq!(player.position_us(), 99 * FRAME_US);
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_seek_while_paused_flushes_the_sink_and_pushes_the_target_frame_once() {
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let at = Arc::new(AtomicI64::new(0));
  let player = open_texture(open_reader(&kf_path()), Box::new(StubSink(log.clone())), stepped_clock(at), None)
    .expect("open texture player");
  let target = 1_600_000;
  player.seek(target);
  wait_until("the target frame", || !pushed(&log).is_empty());
  thread::sleep(Duration::from_millis(100));
  let frames = pushed(&log);
  assert_eq!(frames.len(), 1, "one frame while paused");
  assert_eq!(frames[0].0, target);
  assert_eq!(lock(&log).flushed, 1, "the seek flushed the sink before the target frame");
  assert!(lock(&log).playing.is_empty(), "a step while paused starts no releases");
  // Play continues from the target.
  player.play();
  wait_until("playback after the seek", || pushed(&log).len() >= 3);
  let frames = pushed(&log);
  assert_eq!(frames[1].0, target + FRAME_US);
  assert_eq!(frames[2].0, target + 2 * FRAME_US);
}

#[cfg(not(target_os = "android"))]
#[test]
fn a_url_source_plays_through_the_reader() {
  let data = Arc::new(std::fs::read(gop_path()).expect("read fixture"));
  let (addr, _) = serve(move |req| {
    let start = req.range_start();
    super::source::Script {
      head: super::source::head(
        "206 Partial Content",
        &[
          ("content-range", format!("bytes {start}-{}/{}", data.len() - 1, data.len())),
          ("content-length", (data.len() - start).to_string()),
        ],
      ),
      steps: vec![super::source::Step::Send(data[start..].to_vec())],
    }
  });
  let log = Arc::new(Mutex::new(SinkLog::default()));
  let at = Arc::new(AtomicI64::new(0));
  let reader = open_reader(&format!("http://{addr}/clip.webm"));
  // No audio sink: the track is dropped at the source and the video plays
  // silent.
  let player =
    open_texture(reader, Box::new(StubSink(log.clone())), stepped_clock(at), None).expect("open texture player");
  player.play();
  wait_until("the stream to finish", || player.finished());
  let frames = pushed(&log);
  assert_eq!(frames.len(), 100);
  assert!(frames.windows(2).all(|w| w[1].0 == w[0].0 + FRAME_US), "in order with no gap");
  assert!(player.error().is_none());
}
