// The reader thread over a stalling scripted server and over a file: a
// stall shows as an empty queue and never blocks the consumer, close
// returns promptly during one, a seek starts a fresh epoch measured from
// its target, an unplayed track never queues, and open failures carry
// their kind.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use super::source::{head, lock, serve, Req, Script, Step};
use crate::source::Source;
use crate::video::reader::{Next, Reader, Status};
use crate::video::{ErrorKind, MediaInfo};

/// How long a close may take to return during a stall.
const PROMPT: Duration = Duration::from_secs(2);
/// How long to wait for the reader to deliver or settle.
const PATIENCE: Duration = Duration::from_secs(10);
/// A queue empty for this long is a stalled source, not a slow thread.
const QUIET: Duration = Duration::from_millis(300);
/// The fixture's Opus packets are 20 ms.
const OPUS_PACKET_US: i64 = 20_000;

fn gop_path() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_gop.webm").to_string()
}

/// A Range-honouring server for `data` that stops sending at byte
/// `stall_at` until `resume` is notified (once per connection).
fn stalling(data: Arc<Vec<u8>>, stall_at: usize, resume: Arc<Notify>) -> impl Fn(&Req) -> Script + Send + Sync {
  move |req| {
    let start = req.range_start();
    let body = data[start..].to_vec();
    let steps = if start < stall_at {
      let (first, rest) = body.split_at(stall_at - start);
      vec![Step::Send(first.to_vec()), Step::Wait(resume.clone()), Step::Send(rest.to_vec())]
    } else {
      vec![Step::Send(body.clone())]
    };
    Script {
      head: head(
        "206 Partial Content",
        &[
          ("content-range", format!("bytes {start}-{}/{}", data.len() - 1, data.len())),
          ("content-length", body.len().to_string()),
          ("etag", "\"gop\"".to_string()),
        ],
      ),
      steps,
    }
  }
}

fn open(spec: &str) -> (Reader, Result<MediaInfo, crate::video::StreamError>) {
  let source = Source::parse(spec).expect("a path or URL");
  let (reader, opened) = Reader::open(Box::new(move || source.open("forge-test")));
  let info = opened.blocking_recv().expect("the reader answers the open");
  (reader, info)
}

/// Video packets until the queue has been empty for QUIET, or the end.
fn drain_video(reader: &Reader) -> (Vec<(i64, bool)>, bool) {
  let mut out = Vec::new();
  let mut quiet_since = Instant::now();
  loop {
    match reader.next_video() {
      Next::Packet(au) => {
        out.push((au.pts_us, au.sync));
        quiet_since = Instant::now();
      }
      Next::End => return (out, true),
      Next::Waiting => {
        if quiet_since.elapsed() > QUIET {
          return (out, false);
        }
        std::thread::sleep(Duration::from_millis(5));
      }
    }
  }
}

fn wait_until(reader: &Reader, what: &str, done: impl Fn(&Status) -> bool) -> Status {
  let deadline = Instant::now() + PATIENCE;
  loop {
    let status = reader.status();
    if done(&status) {
      return status;
    }
    assert!(Instant::now() < deadline, "timed out waiting for {what}: {status:?}");
    std::thread::sleep(Duration::from_millis(5));
  }
}

#[test]
fn a_stalled_source_shows_as_an_empty_queue_and_close_returns_promptly() {
  let data = Arc::new(std::fs::read(gop_path()).expect("read fixture"));
  let resume = Arc::new(Notify::new());
  let stall_at = data.len() / 2;
  let (addr, _) = serve(stalling(data.clone(), stall_at, resume.clone()));
  let url = format!("http://{addr}/clip.webm");

  // The header and the first keyframe come from before the stall.
  let (reader, info) = open(&url);
  let info = info.expect("open");
  assert!(info.seekable);
  assert_eq!(info.duration_us.map(|d| d / 1_000_000), Some(4));

  // What the server sent before the stall comes out, then the queue is
  // empty and stays empty: not an end, not an error.
  let (before, ended) = drain_video(&reader);
  assert!(!ended);
  assert!(!before.is_empty() && before.len() < 100, "{} frames before the stall", before.len());
  assert_eq!(before[0], (0, true));
  let status = reader.status();
  assert!(!status.ended && status.error.is_none(), "{status:?}");

  // Closing during the stall returns at once: the blocked read is
  // interrupted and the source cancelled.
  let started = Instant::now();
  drop(reader);
  assert!(started.elapsed() < PROMPT, "close took {:?}", started.elapsed());

  // A second reader rides out the same stall to the end.
  let (reader, info) = open(&url);
  info.expect("open again");
  let (first, ended) = drain_video(&reader);
  assert!(!ended);
  resume.notify_waiters();
  resume.notify_one();
  let deadline = Instant::now() + PATIENCE;
  let mut frames = first;
  loop {
    let (more, ended) = drain_video(&reader);
    frames.extend(more);
    if ended {
      break;
    }
    assert!(Instant::now() < deadline, "the stream never ended after the stall ({} frames)", frames.len());
  }
  assert_eq!(frames.len(), 100, "4 s at 25 fps");
  assert!(frames.windows(2).all(|w| w[1].0 == w[0].0 + 40_000), "frames in order with no gap");
  let mut audio = 0;
  while let Next::Packet(_) = reader.next_audio() {
    audio += 1;
  }
  assert!(audio > 150, "{audio} audio packets");
  let status = reader.status();
  assert!(status.ended && status.error.is_none(), "{status:?}");
}

#[test]
fn a_seek_starts_a_fresh_epoch_measured_from_its_target() {
  let (reader, info) = open(&gop_path());
  let info = info.expect("open");
  let epoch = reader.status().epoch;
  // Let the whole clip queue up (it is shorter than the read-ahead), then
  // seek forward: the queues empty at once, the epoch's resume position
  // is the target (a keyframe), and the lead counts from the target, not
  // from the head left behind at the start.
  wait_until(&reader, "the end of the clip", |s| s.ended);
  assert!(reader.status().lead_us > 3_000_000);
  assert_eq!(reader.seek(3_000_000), epoch + 1);
  let cleared = reader.status();
  assert_eq!(cleared.lead_us, 0);
  assert!(!cleared.ended);
  let landed = wait_until(&reader, "the seek to land", |s| s.resume_us.is_some());
  assert_eq!(landed.resume_us, Some(3_000_000));
  let (frames, ended) = {
    let deadline = Instant::now() + PATIENCE;
    loop {
      let (frames, ended) = drain_video(&reader);
      if ended {
        break (frames, ended);
      }
      assert!(frames.is_empty() || Instant::now() < deadline, "the epoch never ended");
    }
  };
  assert!(ended);
  assert_eq!(frames.first(), Some(&(3_000_000, true)), "the epoch opens on the keyframe at the target");
  assert_eq!(frames.len(), 25, "the last second");
  let status = reader.status();
  assert_eq!(status.epoch, epoch + 1);
  // The last second, to the audio track's final packet a frame past it.
  assert!(status.lead_us <= 1_000_000 + 40_000, "lead from the target: {}", status.lead_us);
  // Audio resumes from the preroll before the target, or from the target
  // itself when the keyframe opens its cluster (the preroll packets sit in
  // the cluster before, which the seek does not visit).
  let preroll_us = info.audio.as_ref().map_or(0, |a| a.seek_preroll_us);
  let Next::Packet(first_audio) = reader.next_audio() else { panic!("audio after the seek") };
  assert!(
    first_audio.pts_us >= 3_000_000 - preroll_us && first_audio.pts_us <= 3_000_000 + OPUS_PACKET_US,
    "{}",
    first_audio.pts_us
  );
  // Releasing a frame moves the head, and the lead with it.
  reader.released(3_500_000);
  assert!(reader.status().lead_us <= 500_000 + OPUS_PACKET_US);
}

#[test]
fn an_unplayed_audio_track_never_queues() {
  let (reader, info) = open(&gop_path());
  assert!(info.expect("open").audio.is_some());
  reader.set_tracks(true, false);
  let (frames, ended) = {
    let deadline = Instant::now() + PATIENCE;
    loop {
      let (frames, ended) = drain_video(&reader);
      if ended {
        break (frames, ended);
      }
      assert!(Instant::now() < deadline, "the clip never ended");
    }
  };
  assert!(ended);
  assert_eq!(frames.len(), 100);
  assert_eq!(reader.status().audio_queued, 0);
  assert!(matches!(reader.next_audio(), Next::End));
}

#[test]
fn open_failures_carry_their_kind() {
  // A server answering 404: the source could not be opened.
  let (addr, _) =
    serve(|_| Script { head: head("404 Not Found", &[("content-length", "0".to_string())]), steps: vec![] });
  let (_reader, info) = open(&format!("http://{addr}/missing.webm"));
  let err = info.expect_err("a 404 fails the open");
  assert_eq!(err.kind, ErrorKind::Network, "{err}");

  // A file that is not WebM: unsupported.
  let (_reader, info) = open(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
  let err = info.expect_err("a manifest is not a stream");
  assert_eq!(err.kind, ErrorKind::Unsupported, "{err}");

  // A missing file: the source could not be opened.
  let (_reader, info) = open("/nowhere/clip.webm");
  let err = info.expect_err("a missing file fails the open");
  assert_eq!(err.kind, ErrorKind::Network, "{err}");
}

#[test]
fn an_abandoned_open_stops_the_reader() {
  let data = Arc::new(std::fs::read(gop_path()).expect("read fixture"));
  let resume = Arc::new(Notify::new());
  // Stall inside the header: the open cannot answer until the server does.
  let (addr, requests) = serve(stalling(data.clone(), 100, resume.clone()));
  let source = Source::parse(&format!("http://{addr}/clip.webm")).expect("url");
  let (reader, opened) = Reader::open(Box::new(move || source.open("forge-test")));
  std::thread::sleep(QUIET);
  assert_eq!(lock(&requests).len(), 1, "the request went out");
  // Dropping the reader (an aborted open, a disposed owner) returns at
  // once with the server still silent.
  drop(opened);
  let started = Instant::now();
  drop(reader);
  assert!(started.elapsed() < PROMPT, "close took {:?}", started.elapsed());
}
