// The playback worker over a stub presenter and a fixture reader: the
// release policy end to end against an injected clock, first-frame
// anchoring with and without a sink, seek (the preroll skipped, one release
// while paused), buffering over a stalling source, close returning at once
// with the presenter dropped on the worker, and the exit flag after every
// way playback can end.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use super::reader::stalling;
use super::source::serve;
use super::video::{FakeSink, FakeSinkState};
use crate::source::Source;
use crate::video::audio::AUDIO_OUTPUT_LATENCY_US;
use crate::video::reader::{Next, Reader};
use crate::video::transport::{monotonic_ns, AudioSink, Clock, DROP_LATE_NS, RELEASE_LEAD_NS, STALL_REANCHOR_NS};
use crate::video::worker::{Feed, LostSampler, Picture, Player, Presenter, PresenterHost};
use crate::video::StreamError;

/// How long to wait for the worker to reach a state.
const PATIENCE: Duration = Duration::from_secs(10);
/// How long a close may take to return, and the worker to exit after it.
const PROMPT: Duration = Duration::from_millis(500);
/// Scheduling noise allowed on a timed release, in nanoseconds: how far
/// ahead of the lead a frame may be handed over (further is the schedule
/// running ahead, the bug these tests exist to catch), and the measurement
/// slack on a held audio start.
const SLACK_NS: i64 = 15_000_000;
/// How late the host may wake the sleeping worker, in nanoseconds: a frame
/// handed over that much after its lead is the runner's scheduler, not the
/// policy (the shared macOS CI runner has missed by 58 ms with the suite
/// single-threaded). The policy's own lateness is judged by the
/// stepped-clock test, which has no host in the loop; this bound only
/// catches a worker that stalls.
const HOST_WAKE_LATE_NS: i64 = 150_000_000;
/// Pictures the stub holds decoded ahead of the worker, like a small codec.
const STUB_QUEUE: usize = 2;
/// What the stub charges per picture, so a reader over a file stays ahead
/// of it the way a reader stays ahead of a real decoder (an instant decoder
/// would catch the demuxer up mid-file and trip the buffering rule).
const STUB_DECODE: Duration = Duration::from_millis(1);
/// The fixtures' frame interval (25 fps).
const FRAME_US: i64 = 40_000;

// 4 s of ffmpeg testsrc2 160x120 at 25 fps, no audio, a keyframe every
// second: 100 frames, seekable.
fn kf_path() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_kf.webm").to_string()
}

// 2 s of the same with a 440 Hz mono Opus track: 50 frames, one keyframe.
fn av_path() -> String {
  concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests/data/video_av.webm").to_string()
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

/// What the stub saw: every release (pts, the release time asked for, the
/// clock reading at the call), every discard, and where it was dropped.
#[derive(Default)]
struct Log {
  released: Vec<(i64, i64, i64)>,
  discarded: Vec<Picture>,
  dropped_on: Option<ThreadId>,
}

/// A presenter that decodes nothing: it takes packets from the reader into
/// a small queue and hands their pts back as pictures, so the tests see
/// the worker's decisions alone. Records into `log`; `now` is the clock the
/// worker schedules on, read at each release.
struct StubPresenter {
  queue: VecDeque<Picture>,
  current: Option<Picture>,
  log: Arc<Mutex<Log>>,
  now: Arc<dyn Fn() -> i64 + Send + Sync>,
  lead_ns: i64,
  // Fail `next` once this many pictures have been released.
  fail_after: Option<usize>,
}

impl Presenter for StubPresenter {
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError> {
    if self.queue.len() >= STUB_QUEUE {
      return Ok(Feed::Full);
    }
    match reader.next_video() {
      Next::Packet(au) => {
        self.queue.push_back(Picture { pts_us: au.pts_us, eos: false, empty: false });
        Ok(Feed::Took)
      }
      Next::Waiting => Ok(Feed::Waiting),
      Next::End => {
        // A codec's end of stream comes out as an empty buffer after the
        // last picture.
        self.queue.push_back(Picture { pts_us: 0, eos: true, empty: true });
        Ok(Feed::End)
      }
    }
  }

  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError> {
    if self.fail_after.is_some_and(|n| lock(&self.log).released.len() >= n) {
      return Err(StreamError::decode("stub decoder gave up"));
    }
    match self.queue.pop_front() {
      Some(picture) => {
        thread::sleep(STUB_DECODE);
        self.current = Some(picture);
        Ok(Some(picture))
      }
      None => {
        thread::sleep(wait);
        Ok(None)
      }
    }
  }

  fn snap(&self, due_ns: i64) -> i64 {
    due_ns
  }

  fn release_at(&mut self, release_ns: i64) -> Result<(), String> {
    let picture = self.current.take().ok_or("no picture is current")?;
    lock(&self.log).released.push((picture.pts_us, release_ns, (self.now)()));
    Ok(())
  }

  fn discard(&mut self) -> Result<(), String> {
    let picture = self.current.take().ok_or("no picture is current")?;
    lock(&self.log).discarded.push(picture);
    Ok(())
  }

  fn flush(&mut self) -> Result<(), String> {
    self.queue.clear();
    self.current = None;
    Ok(())
  }

  fn lead_ns(&self) -> i64 {
    self.lead_ns
  }
}

impl PresenterHost for StubPresenter {
  fn presenter(&mut self) -> Box<dyn Presenter + '_> {
    Box::new(self)
  }
}

impl Drop for StubPresenter {
  fn drop(&mut self) {
    lock(&self.log).dropped_on = Some(thread::current().id());
  }
}

/// A clock the test steps by hand (headless playback's shape).
fn stepped_clock(at: Arc<AtomicI64>) -> (Clock, Arc<dyn Fn() -> i64 + Send + Sync>) {
  let read: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(move || at.load(Ordering::Relaxed));
  let now = read.clone();
  (Clock { now_ns: Box::new(move || now()), stepped: true }, read)
}

/// The system clock plus an offset the test bumps (a stall, a jump).
fn shifted_clock(offset: Arc<AtomicI64>) -> (Clock, Arc<dyn Fn() -> i64 + Send + Sync>) {
  let read: Arc<dyn Fn() -> i64 + Send + Sync> = Arc::new(move || monotonic_ns() + offset.load(Ordering::Relaxed));
  let now = read.clone();
  (Clock { now_ns: Box::new(move || now()), stepped: false }, read)
}

struct Rig {
  player: Player,
  log: Arc<Mutex<Log>>,
}

fn never_lost() -> LostSampler {
  Box::new(|| false)
}

fn open_stub(
  reader: Reader,
  clock: Clock,
  now: Arc<dyn Fn() -> i64 + Send + Sync>,
  lead_ns: i64,
  sink: Option<Box<dyn AudioSink>>,
  lost: LostSampler,
  fail_after: Option<usize>,
) -> Rig {
  let log = Arc::new(Mutex::new(Log::default()));
  let stub = StubPresenter { queue: VecDeque::new(), current: None, log: log.clone(), now, lead_ns, fail_after };
  let make = Box::new(move || Ok(Box::new(stub) as Box<dyn PresenterHost>));
  let player = Player::open("srt-video-test", reader, make, clock, sink, lost).expect("open player");
  Rig { player, log }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
  let deadline = Instant::now() + PATIENCE;
  while !done() {
    assert!(Instant::now() < deadline, "timed out waiting for {what}");
    thread::sleep(Duration::from_millis(1));
  }
}

/// `wait_until` with the rig's log in the failure, for the timing tests.
fn wait_on(rig: &Rig, what: &str, mut done: impl FnMut() -> bool) {
  let deadline = Instant::now() + PATIENCE;
  while !done() {
    if Instant::now() >= deadline {
      let log = lock(&rig.log);
      panic!("timed out waiting for {what}: released {:?}, discarded {:?}", log.released, log.discarded);
    }
    thread::sleep(Duration::from_millis(1));
  }
}

fn released(rig: &Rig) -> Vec<(i64, i64, i64)> {
  lock(&rig.log).released.clone()
}

/// Whether a frame handed over `early` ns before its release time was
/// released on the given lead: never further ahead than the lead (plus
/// slack), and late only by what the host's wake-up may miss
/// (HOST_WAKE_LATE_NS). A late wake hands the frame in hand over late
/// whatever the policy does, and production tolerates that by design.
fn on_lead(early: i64, lead: i64) -> bool {
  early - lead <= SLACK_NS && early >= -HOST_WAKE_LATE_NS
}

fn exits_promptly(rig: &Rig) {
  let shared = rig.player.shared();
  let deadline = Instant::now() + PROMPT;
  while !shared.has_exited() {
    assert!(Instant::now() < deadline, "the worker did not exit within {PROMPT:?}");
    thread::sleep(Duration::from_millis(1));
  }
}

#[test]
fn a_stepped_clock_releases_every_frame_at_the_content_interval() {
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(open_reader(&kf_path()), clock, now, RELEASE_LEAD_NS, None, never_lost(), None);
  assert!(!rig.player.playing());
  rig.player.play();
  wait_until("the stream to finish", || rig.player.finished());
  let released = released(&rig);
  assert_eq!(released.len(), 100, "4 s at 25 fps");
  let (first_pts, first_ns, _) = released[0];
  assert_eq!(first_pts, 0);
  assert_eq!(first_ns, 0, "the first frame anchors on the clock's reading");
  for (k, &(pts, release_ns, _)) in released.iter().enumerate() {
    assert_eq!(pts, k as i64 * FRAME_US);
    assert_eq!(release_ns - first_ns, k as i64 * FRAME_US * 1000, "frame {k} due on the anchor, never waited for");
  }
  // Only the end-of-stream buffer was discarded.
  let discarded = lock(&rig.log).discarded.clone();
  assert_eq!(discarded.len(), 1);
  assert!(discarded[0].empty && discarded[0].eos);
  assert_eq!(rig.player.position_us(), 99 * FRAME_US);
  assert!(!rig.player.shared().has_exited(), "an ended stream parks, it does not exit");
  rig.player.close();
  exits_promptly(&rig);
}

#[test]
fn the_release_policy_waits_drops_and_reanchors_against_the_clock() {
  let offset = Arc::new(AtomicI64::new(0));
  let (clock, now) = shifted_clock(offset.clone());
  let lead = RELEASE_LEAD_NS;
  let frame_ns = FRAME_US * 1000;
  let rig = open_stub(open_reader(&kf_path()), clock, now, lead, None, never_lost(), None);
  let dropped = || lock(&rig.log).discarded.iter().filter(|p| !p.empty).count();
  rig.player.play();
  // Once the schedule runs ahead of the lead, every frame is handed over
  // one lead before its time (the first couple go at once: the anchor puts
  // them within the lead), never earlier; a host that wakes the worker
  // late hands it over later, still within the release window.
  wait_on(&rig, "ten releases", || released(&rig).len() >= 10);
  let released_now = released(&rig);
  for &(pts, release_ns, now_ns) in &released_now[3..10] {
    let early = release_ns - now_ns;
    assert!(on_lead(early, lead), "frame {pts}us handed over {early}ns early, lead {lead}ns");
  }
  // A slow host may already have dropped a frame; the jump is judged by
  // the drops it adds.
  let dropped_before_jump = dropped();

  // The clock jumps ahead by more than the lead, the drop threshold and a
  // frame (the frame in hand is released at its time whatever the clock
  // did, and the lead absorbs a jump smaller than itself) and less than the
  // stall threshold: decode is "behind", the late frames are dropped until
  // the schedule catches up, then releases resume on the same anchor.
  let jump = lead + DROP_LATE_NS + 3 * frame_ns;
  assert!(jump < STALL_REANCHOR_NS);
  offset.fetch_add(jump, Ordering::Relaxed);
  wait_on(&rig, "a late frame to be dropped", || dropped() > dropped_before_jump);
  let seen = released(&rig).len();
  wait_on(&rig, "releases to resume", || released(&rig).len() >= seen + 3);
  let count = dropped() - dropped_before_jump;
  assert!(count <= (jump / frame_ns) as usize + 1, "{count} frames dropped for a {jump}ns jump");
  let resumed = released(&rig);
  let (_, release_ns, now_ns) = resumed[resumed.len() - 1];
  assert!(on_lead(release_ns - now_ns, lead), "the schedule continues on the anchor after the drops");

  // A jump past the stall threshold re-anchors instead: the next frame is
  // released for now, not dropped, and the frames after it wait again.
  let before = released(&rig).len();
  let dropped_before = dropped();
  offset.fetch_add(STALL_REANCHOR_NS * 2, Ordering::Relaxed);
  wait_on(&rig, "releases after the stall", || released(&rig).len() >= before + 4);
  let after = released(&rig);
  // The frame in hand at the jump goes out at its old time; the one after
  // it is the re-anchored one.
  let reanchored = after[before..]
    .iter()
    .position(|&(_, release_ns, now_ns)| on_lead(release_ns - now_ns, 0))
    .map(|k| before + k)
    .unwrap_or_else(|| panic!("no frame was released for now after the stall: {:?}", &after[before..]));
  assert!(reanchored <= before + 1, "the re-anchor came {} frames after the jump", reanchored - before);
  assert_eq!(dropped(), dropped_before, "a stall drops nothing");
  let (_, release_ns, now_ns) = after[reanchored + 2];
  assert!(on_lead(release_ns - now_ns, lead), "after the re-anchor the lead holds again");
}

#[test]
fn a_seek_while_paused_shows_the_target_frame_once_and_skips_the_preroll() {
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(open_reader(&kf_path()), clock, now, RELEASE_LEAD_NS, None, never_lost(), None);
  // Paused: the seek decodes from the keyframe at 1 s, discards up to the
  // target and shows the target frame, once.
  let target = 1_600_000;
  rig.player.seek(target);
  wait_until("the target frame", || !released(&rig).is_empty());
  thread::sleep(Duration::from_millis(100));
  let released_now = released(&rig);
  assert_eq!(released_now.len(), 1, "one frame while paused");
  assert_eq!(released_now[0].0, target);
  let preroll: Vec<i64> = lock(&rig.log).discarded.iter().map(|p| p.pts_us).collect();
  let expected: Vec<i64> = (0..((target - 1_000_000) / FRAME_US)).map(|k| 1_000_000 + k * FRAME_US).collect();
  assert_eq!(preroll, expected, "the frames from the keyframe to the target are decoded and discarded");
  assert_eq!(rig.player.position_us(), target);
  assert!(!rig.player.playing());
  // Play continues from the target.
  rig.player.play();
  wait_until("playback after the seek", || released(&rig).len() >= 3);
  let after = released(&rig);
  assert_eq!(after[1].0, target + FRAME_US);
  assert_eq!(after[2].0, target + 2 * FRAME_US);
  // A seek while playing lands on its target too, forward and back.
  rig.player.seek(200_000);
  wait_until("the backward seek", || released(&rig).iter().any(|&(pts, _, _)| pts == 200_000));
  let all = released(&rig);
  let at_target = all.iter().position(|&(pts, _, _)| pts == 200_000).expect("the target was released");
  assert!(all[at_target - 1].0 > 200_000, "the frames before the seek came from later in the clip");
}

#[test]
fn the_first_frame_anchors_on_the_sound_when_a_sink_consumes() {
  let offset = Arc::new(AtomicI64::new(0));
  let (clock, now) = shifted_clock(offset);
  let state = Arc::new(Mutex::new(FakeSinkState::default()));
  let sink: Box<dyn AudioSink> = Box::new(FakeSink(state.clone()));
  let rig = open_stub(open_reader(&av_path()), clock, now, RELEASE_LEAD_NS, Some(sink), never_lost(), None);
  // A device that consumes at the track's rate once the track is playing.
  let device = state.clone();
  let stop = Arc::new(AtomicBool::new(false));
  let stopped = stop.clone();
  let consumer = thread::spawn(move || {
    while !stopped.load(Ordering::Relaxed) {
      thread::sleep(Duration::from_millis(1));
      let mut s = lock(&device);
      if !s.paused {
        s.consume(48);
      }
    }
  });
  let play_ns = monotonic_ns();
  rig.player.play();
  wait_until("two releases", || released(&rig).len() >= 2);
  let released_now = released(&rig);
  let (pts, first_ns, _) = released_now[0];
  assert_eq!(pts, 0);
  // The picture is held back by the output latency the sound's clock
  // reports: content time 0 is due once the sink has consumed the
  // latency's worth, not at the instant play was called.
  let held_ns = first_ns - play_ns;
  let latency_ns = AUDIO_OUTPUT_LATENCY_US * 1000;
  assert!(
    held_ns >= latency_ns - SLACK_NS,
    "first frame due {held_ns}ns after play, expected the {latency_ns}ns latency"
  );
  assert!(held_ns < latency_ns + PROMPT.as_nanos() as i64, "first frame due {held_ns}ns after play");
  assert_eq!(released_now[1].1 - first_ns, FRAME_US * 1000, "the second frame follows the anchor");
  assert!(lock(&state).pushed_frames > 0, "the track was fed");
  stop.store(true, Ordering::Relaxed);
  consumer.join().expect("consumer");

  // Without a sink the first frame anchors on itself: due at once.
  let offset = Arc::new(AtomicI64::new(0));
  let (clock, now) = shifted_clock(offset);
  let silent = open_stub(open_reader(&av_path()), clock, now, RELEASE_LEAD_NS, None, never_lost(), None);
  let play_ns = monotonic_ns();
  silent.player.play();
  wait_until("a release", || !released(&silent).is_empty());
  let (_, first_ns, _) = released(&silent)[0];
  assert!(first_ns - play_ns < SLACK_NS, "first frame due {}ns after play", first_ns - play_ns);
}

#[test]
fn buffering_starts_on_a_stall_and_ends_when_the_source_resumes() {
  let data = Arc::new(std::fs::read(gop_path()).expect("read fixture"));
  let resume = Arc::new(Notify::new());
  let stall_at = data.len() / 2;
  let (addr, _) = serve(stalling(data.clone(), stall_at, resume.clone()));
  let reader = open_reader(&format!("http://{addr}/clip.webm"));
  // A stepped clock releases every frame at once, so the worker eats
  // through whatever the source delivered and runs into the stall.
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(reader, clock, now, RELEASE_LEAD_NS, None, never_lost(), None);
  rig.player.play();
  wait_until("buffering to start", || rig.player.buffering());
  assert!(rig.player.playing(), "buffering is a held play, not a pause");
  let held = released(&rig).len();
  assert!(held > 0 && held < 100, "{held} frames released before the stall");
  thread::sleep(Duration::from_millis(100));
  assert_eq!(released(&rig).len(), held, "nothing is released while buffering");
  resume.notify_waiters();
  resume.notify_one();
  wait_until("buffering to end", || !rig.player.buffering());
  wait_until("the stream to finish", || rig.player.finished());
  let all = released(&rig);
  assert_eq!(all.len(), 100, "every frame after the stall");
  assert!(all.windows(2).all(|w| w[1].0 == w[0].0 + FRAME_US), "in order with no gap");
  // The resume re-anchored: the frames after the stall are due from the
  // clock's reading again, not from the stalled schedule.
  assert_eq!(all[held].1, 0, "the first frame after buffering anchors afresh");
}

#[test]
fn close_during_a_wait_returns_at_once_and_drops_the_presenter_on_the_worker() {
  let offset = Arc::new(AtomicI64::new(0));
  let (clock, now) = shifted_clock(offset);
  // No lead: every frame waits its whole interval, so close lands inside a
  // wait.
  let rig = open_stub(open_reader(&kf_path()), clock, now, 0, None, never_lost(), None);
  rig.player.play();
  wait_until("three releases", || released(&rig).len() >= 3);
  let started = Instant::now();
  rig.player.close();
  let took = started.elapsed();
  assert!(took < Duration::from_millis(20), "close took {took:?}");
  exits_promptly(&rig);
  let dropped_on = lock(&rig.log).dropped_on.expect("the presenter was dropped");
  assert_ne!(dropped_on, thread::current().id(), "the presenter dropped on the worker, not the caller");
  // Commands after the exit are no-ops.
  rig.player.play();
  rig.player.seek(0);
}

#[test]
fn the_exit_follows_close_from_the_ended_failed_and_lost_states_and_a_drop() {
  // Ended.
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(open_reader(&kf_path()), clock, now, RELEASE_LEAD_NS, None, never_lost(), None);
  rig.player.play();
  wait_until("the end", || rig.player.finished());
  rig.player.close();
  exits_promptly(&rig);

  // Failed: the presenter gives up mid-stream, playback stops with the
  // error published, and close still exits.
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(open_reader(&kf_path()), clock, now, RELEASE_LEAD_NS, None, never_lost(), Some(5));
  rig.player.play();
  wait_until("the failure", || rig.player.error().is_some());
  assert!(!rig.player.playing());
  assert!(!rig.player.finished(), "a failure is not the end");
  assert_eq!(released(&rig).len(), 5);
  rig.player.close();
  exits_promptly(&rig);

  // Lost: the platform took the surface; playback finishes.
  let lost = Arc::new(AtomicBool::new(false));
  let sample = lost.clone();
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(
    open_reader(&kf_path()),
    clock,
    now,
    RELEASE_LEAD_NS,
    None,
    Box::new(move || sample.load(Ordering::Relaxed)),
    None,
  );
  rig.player.play();
  wait_until("some playback", || released(&rig).len() >= 2);
  lost.store(true, Ordering::Relaxed);
  wait_until("the lost surface to finish playback", || rig.player.finished());
  rig.player.close();
  exits_promptly(&rig);

  // Dropping the player is the close.
  let at = Arc::new(AtomicI64::new(0));
  let (clock, now) = stepped_clock(at);
  let rig = open_stub(open_reader(&kf_path()), clock, now, RELEASE_LEAD_NS, None, never_lost(), None);
  let shared = rig.player.shared();
  let log = rig.log.clone();
  drop(rig);
  let deadline = Instant::now() + PROMPT;
  while !shared.has_exited() {
    assert!(Instant::now() < deadline, "the worker did not exit after the drop");
    thread::sleep(Duration::from_millis(1));
  }
  assert!(lock(&log).dropped_on.is_some());
}

#[test]
fn a_presenter_that_cannot_be_built_ends_the_stream_and_exits() {
  let make = Box::new(|| Err("no decoder on this box".to_string()));
  let player = Player::open("srt-video-test", open_reader(&kf_path()), make, Clock::monotonic(), None, never_lost())
    .expect("open player");
  wait_until("the failure", || player.finished());
  let error = player.error().expect("the reason is published");
  assert!(error.message.contains("no decoder"), "{error}");
  let shared = player.shared();
  let deadline = Instant::now() + PROMPT;
  while !shared.has_exited() {
    assert!(Instant::now() < deadline, "the worker did not exit");
    thread::sleep(Duration::from_millis(1));
  }
}
