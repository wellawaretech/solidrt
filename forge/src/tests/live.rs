//! The live form end to end: a recorder writing into its live sink, the
//! stream read as a media source by a reader under the live policy, and
//! the texture player over it. Frames reach the sink as they are made and
//! the player ends when the recording stops; a consumer that does not keep
//! up (a paused player) sees the reader cut its backlog to the newest
//! keyframe inside the latency.

use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::source::from_stream;
use crate::video::reader::{Live, Next, Reader, LIVE_CATCH_UP_MARGIN_US};
use crate::video::transport::{Clock, FrameSink};
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
// Frames the catch-up test feeds a paused player: past two keyframes (at
// 2 s and 4 s), so the cut lands on the second.
const CATCH_UP_FRAMES: usize = 121;
const SECOND_KEYFRAME_US: i64 = 4_000_000;
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

/// A live recorder and the reader over its output, under the live policy.
fn open_live() -> (Recorder, Reader) {
  open_live_with(false)
}

/// `open_live`, the stream re-cut at `TRANSPORT_CHUNK` boundaries when
/// `rechunk`.
fn open_live_with(rechunk: bool) -> (Recorder, Reader) {
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
  let (reader, opened) = Reader::open_with(Box::new(move || from_stream(stream)), Some(Live::with_latency(LATENCY_US)));
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
  let (recorder, reader) = open_live_with(true);
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
fn a_paused_player_has_its_backlog_cut_to_the_newest_keyframe() {
  let (recorder, reader) = open_live();
  let live = reader.live().expect("a live reader");
  // A burst: nobody releases frames, the lead grows past the resume buffer
  // and the margin, and the reader cuts to the keyframes as they come, the
  // last one at 4 s.
  feed(&recorder, 1, CATCH_UP_FRAMES, false);
  wait_until("the catch-up to reach the second keyframe", || reader.status().resume_us == Some(SECOND_KEYFRAME_US));
  let status = reader.status();
  assert!(status.epoch >= 1, "an epoch per cut");
  assert!(status.lead_us <= live.resume_us + LIVE_CATCH_UP_MARGIN_US, "the lead is back inside the latency");
  match reader.next_video() {
    Next::Packet(au) => assert!(au.sync && au.pts_us == SECOND_KEYFRAME_US, "the queue starts at the keyframe"),
    _ => panic!("a packet is queued"),
  }
  recorder.stop().expect("stop");
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
