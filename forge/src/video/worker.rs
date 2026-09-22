// The one playback worker, shared by every player: one thread that owns
// the presenter (the codec on a surface, a buffer-mode codec over a frame
// sink, or a software decoder over one), the audio track and the transport
// anchor, takes packets from the reader thread's queues (reader.rs: the only
// thread that touches the source), drains commands between frames, and
// publishes position/playing/finished/buffering/error. It never blocks the
// caller: the presenter is built on the worker (a hardware codec may wait
// for the instance the previous player has yet to release), the caller
// reads state from atomics, and close sends a command and returns - the
// worker drops everything it owned on its way out and marks the exit
// (okf/plans/video-texture-off-frame-loop.md).
//
// What differs between a plane, a buffer-mode codec and a software decoder
// is the `Presenter` below: how a coded frame gets in, how a decoded picture
// is made current, and where a released picture goes. The loop, the
// buffering rule, the audio track and the release policy are the same for
// all of them: poll the reader, feed audio, drain commands, feed the
// presenter, judge buffering, present one picture against the clock.
//
// Audio never selects frames. The picture keeps its own clock (the anchor
// on the injected clock, snapped to a vsync grid when the presenter has
// one); the audio track is decoded and pushed ahead into the caller's sink
// between frame releases, and the sink's position only nudges the anchor,
// once, when the smoothed lead crosses a threshold (transport::AudioSync).
// The first frame after play, a seek or buffering waits for the sink to
// consume and anchors on the audio clock, so picture and sound start
// together without a correction.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::audio::AudioTrack;
use super::reader::{Reader, ReaderHandle};
use super::transport::{
  classify, Anchor, AudioSink, AudioSupply, AudioSync, Buffering, Clock, Command, Controls, Release, Shared, Supply,
};
use super::{MediaInfo, StreamError};

// How long one `next` waits when the presenter has nothing ready: the
// worker's idle granularity while playing (commands are drained between
// waits, so this also bounds command latency). Also the pace of a pass
// while buffering, when nothing is presented.
const OUTPUT_WAIT: Duration = Duration::from_millis(10);
// How often a paused or ended worker looks at the reader and the surface,
// so a failure or a lost surface is published without a command.
const IDLE_POLL: Duration = Duration::from_millis(250);
// The presenter accepting no input for this long while playing with a
// packet waiting means it is wedged, not busy (decode itself runs at 3x
// realtime on the slowest target); the stream fails rather than hangs.
const INPUT_STALL: Duration = Duration::from_millis(2000);
// How long the first frame after play, a seek or buffering waits for the
// sink to start consuming before it anchors on itself instead (a device
// that does not resume). Well above a paused device's resume delay, which
// is tens of milliseconds.
const AUDIO_START_TIMEOUT: Duration = Duration::from_millis(300);
// How often that wait looks at the sink's position.
const AUDIO_START_POLL: Duration = Duration::from_millis(2);

/// Where decoded pictures go, and how coded frames get there: a surface
/// MediaCodec renders into (the plane), a buffer-mode codec whose output is
/// copied out and handed to a `FrameSink` (Android textures), or a
/// synchronous decoder doing the same (libvpx). Holds the picture it has
/// made current until it is released or discarded, so the worker never
/// holds a codec buffer across a borrow. Borrowed from its `PresenterHost`
/// for the run, on the worker thread, so a codec handle need not be `Send`.
pub trait Presenter {
  /// Take at most ONE coded frame from the reader when there is room, so
  /// commands drain between frames whatever decode costs. The worker calls
  /// again while `Took` comes back.
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError>;
  /// Make the next decoded picture current, waiting up to `wait` for it.
  /// None when nothing is ready within the wait.
  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError>;
  /// The time the current picture would be released for when asked for
  /// `due_ns`: snapped onto a vsync grid when the presenter has one, `due_ns`
  /// itself otherwise. The worker waits against this, so the hand-over
  /// clears the compositor's deadline for the snapped vsync.
  fn snap(&self, due_ns: i64) -> i64;
  /// Release the current picture for the time `release_ns` (a `snap` result,
  /// on the worker's clock).
  fn release_at(&mut self, release_ns: i64) -> Result<(), String>;
  /// Drop the current picture unshown (a seek preroll frame, a late frame,
  /// an empty end-of-stream buffer).
  fn discard(&mut self) -> Result<(), String>;
  /// Forget everything queued or in flight (a seek).
  fn flush(&mut self) -> Result<(), String>;
  /// How far ahead of its due time a picture must be released: one
  /// compositor period, `RELEASE_LEAD_NS` when the period is unknown.
  fn lead_ns(&self) -> i64;
  /// Pictures are being released (playing, not buffering, not failed, not
  /// at the end) or not: what a frame sink holds the compositor's standing
  /// demand on. Nothing to do for a surface.
  fn set_playing(&mut self, _playing: bool) {}
}

/// What one `feed` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
  /// One coded frame went in.
  Took,
  /// The presenter holds all it can; the next `next` makes room.
  Full,
  /// The codec would not take an input buffer. Normal while paused or
  /// buffering (outputs are held); lasting while frames are released, it is
  /// a wedge, which `packet_waiting` (the reader had a frame for it) lets
  /// the worker time.
  Refused { packet_waiting: bool },
  /// The reader has no packet yet (a source that stalls, a seek in flight).
  Waiting,
  /// The stream's end went in; nothing more until a flush.
  End,
}

/// A decoded picture the presenter has made current.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picture {
  pub pts_us: i64,
  /// The stream's last picture (the end-of-stream flag came out with it).
  pub eos: bool,
  /// Carries no pixels (a bare end-of-stream buffer): discard it.
  pub empty: bool,
}

/// What a presenter is built over and borrows from for the run: the codec,
/// the surface or the sink it owns. Split from `Presenter` because ndk's
/// output buffers borrow the codec they came from, so a presenter holding
/// one across worker calls cannot own that codec itself. Built by the
/// factory and dropped on the worker thread, after the presenter.
pub trait PresenterHost {
  fn presenter(&mut self) -> Box<dyn Presenter + '_>;
}

// A presenter that owns everything it needs is its own host (the texture
// presenters): the run borrows it whole.
impl<P: Presenter + ?Sized> Presenter for &mut P {
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError> {
    (**self).feed(reader)
  }
  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError> {
    (**self).next(wait)
  }
  fn snap(&self, due_ns: i64) -> i64 {
    (**self).snap(due_ns)
  }
  fn release_at(&mut self, release_ns: i64) -> Result<(), String> {
    (**self).release_at(release_ns)
  }
  fn discard(&mut self) -> Result<(), String> {
    (**self).discard()
  }
  fn flush(&mut self) -> Result<(), String> {
    (**self).flush()
  }
  fn lead_ns(&self) -> i64 {
    (**self).lead_ns()
  }
  fn set_playing(&mut self, playing: bool) {
    (**self).set_playing(playing)
  }
}

/// Whether the platform took the surface beneath the presenter (the
/// activity went to the background): sampled every pass, the player then
/// stops and finishes. A texture player never loses its surface.
pub type LostSampler = Box<dyn Fn() -> bool + Send>;

/// Builds the presenter's host ON the worker thread (codec handles are not
/// `Send`, and a hardware codec's construction may wait for a clip
/// handover). A failure ends the stream like any decoder-init failure:
/// `finished` goes true, the reason is logged and published.
pub type PresenterFactory = Box<dyn FnOnce() -> Result<Box<dyn PresenterHost>, String> + Send>;

/// A playing stream: opened over a reader and a presenter, driven through
/// play/pause/seek, read through position/finished/buffering/error. Dropping
/// it (or `close`) cancels the source and sends the worker its close;
/// nothing waits for the worker, which releases what it owns on its way out
/// and sets `exited` on the shared state last.
pub struct Player {
  controls: Controls,
  info: MediaInfo,
  reader: ReaderHandle,
}

impl Player {
  /// Start the worker `name` over the stream `reader` delivers, presenting
  /// through the presenter `make` builds on the worker, scheduling on
  /// `clock`, and playing the audio track (when the stream has one and the
  /// caller provides a `sink` for it, opened at the track's rate and
  /// channels) into that sink. Playback starts paused; the first frame is
  /// shown on `play`. An audio decoder that cannot be created plays silent.
  /// `lost` reports the presenter's surface gone.
  pub fn open(
    name: &str,
    reader: Reader,
    make: PresenterFactory,
    clock: Clock,
    sink: Option<Box<dyn AudioSink>>,
    lost: LostSampler,
  ) -> Result<Player, String> {
    let info = reader.info().ok_or("stream not opened")?;
    let start_us = info.start_us;
    let audio_info = info.audio.clone();
    if audio_info.is_some() && sink.is_none() {
      // Nobody will take the audio packets: drop them at the source.
      reader.set_tracks(true, false);
    }
    let handle = reader.handle();
    let (controls, rx, shared) = Controls::new();
    let worker_shared = shared.clone();
    thread::Builder::new()
      .name(name.to_string())
      .spawn(move || {
        let shared = worker_shared;
        let mut host = match make() {
          Ok(host) => host,
          Err(e) => {
            log::warn!("[forge::video] presenter: {e}");
            shared.set_error(StreamError::decode(e));
            shared.set_finished(true);
            drop(reader);
            shared.set_exited();
            return;
          }
        };
        let audio = match (audio_info, sink) {
          (Some(info), Some(sink)) => match AudioTrack::new(&info, start_us, sink) {
            Ok(track) => Some(track),
            Err(e) => {
              log::warn!("[forge::video] {e} (playing silent)");
              reader.set_tracks(true, false);
              None
            }
          },
          _ => None,
        };
        Worker {
          presenter: host.presenter(),
          reader,
          audio,
          audio_running: false,
          rx,
          shared: shared.clone(),
          clock,
          anchor: Anchor::new(),
          sync: AudioSync::new(),
          lost,
          playing: false,
          input_eos: false,
          output_eos: false,
          failed: false,
          skip_until: None,
          show_one: false,
          epoch: 0,
          resume_pending: false,
          buffering: Buffering::new(),
          input_stalled_since: None,
          pending: VecDeque::new(),
          dropped: 0,
          releasing: false,
        }
        .run();
        // The worker has dropped (the presenter, the audio track with its
        // sink handle, the reader after joining its thread); the host goes
        // now (its codec stopped and deleted, its surface released after
        // it). Only then is the exit published.
        drop(host);
        shared.set_exited();
      })
      .map_err(|e| format!("spawn {name}: {e}"))?;
    Ok(Player { controls, info, reader: handle })
  }

  pub fn info(&self) -> &MediaInfo {
    &self.info
  }

  pub fn play(&self) {
    self.controls.play();
  }

  pub fn pause(&self) {
    self.controls.pause();
  }

  /// Seek to a content time in microseconds. While paused the target frame
  /// is shown; while playing playback continues from it. A no-op on a
  /// source that cannot seek.
  pub fn seek(&self, target_us: i64) {
    if !self.info.seekable {
      return;
    }
    self.controls.seek(target_us);
  }

  pub fn playing(&self) -> bool {
    self.controls.playing()
  }

  /// Presentation time of the last frame released by the worker (handed to
  /// the surface or pushed to the sink).
  pub fn position_us(&self) -> i64 {
    self.controls.position_us()
  }

  /// Whether the stream's last frame has been released.
  pub fn finished(&self) -> bool {
    self.controls.finished()
  }

  /// Whether playback is held for the source to catch up.
  pub fn buffering(&self) -> bool {
    self.controls.buffering()
  }

  /// The failure that stopped playback, if one did.
  pub fn error(&self) -> Option<StreamError> {
    self.controls.error()
  }

  /// The published state, for an async watcher of `failed` or `exited`.
  pub fn shared(&self) -> Arc<Shared> {
    self.controls.shared()
  }

  /// Stop: the source is cancelled (a blocked read returns at once) and the
  /// worker is told to close. Returns without waiting for it; `shared().
  /// exited()` is the wait, for a caller that needs what the worker holds.
  pub fn close(&self) {
    self.reader.close();
    self.controls.close();
  }
}

impl Drop for Player {
  fn drop(&mut self) {
    self.close();
  }
}

struct Worker<'h> {
  presenter: Box<dyn Presenter + 'h>,
  reader: Reader,
  // The audio track when the stream has one and a sink was provided.
  audio: Option<AudioTrack>,
  // The sink is consuming: started when the first frame after play, a seek
  // or buffering is in hand (that frame waits for it, see start_audio),
  // stopped by pause, seek and buffering.
  audio_running: bool,
  rx: Receiver<Command>,
  shared: Arc<Shared>,
  clock: Clock,
  anchor: Anchor,
  // The audio clock's hold on the anchor.
  sync: AudioSync,
  lost: LostSampler,
  playing: bool,
  // End of stream has been fed to the presenter / has come out of it.
  input_eos: bool,
  output_eos: bool,
  // The stream failed: playback stopped, the error is published. A seek
  // retries.
  failed: bool,
  // After a seek: frames before this pts are decoded but not shown (the
  // decode started at the keyframe before the target).
  skip_until: Option<i64>,
  // After a seek while paused: show the first frame at the target, once.
  show_one: bool,
  // The reader epoch this worker consumes, and whether its resume position
  // is still unknown (the seek is in flight): audio is not fed until it is,
  // so the discard point is right.
  epoch: u64,
  resume_pending: bool,
  buffering: Buffering,
  // When the presenter first refused input with a packet waiting.
  input_stalled_since: Option<Instant>,
  // Commands received while waiting for a frame's release time, handled at
  // the top of the next iteration.
  pending: VecDeque<Command>,
  dropped: u64,
  // What the presenter was last told about releases (see
  // `Presenter::set_playing`).
  releasing: bool,
}

impl Worker<'_> {
  fn run(mut self) {
    loop {
      if (self.lost)() {
        self.lose();
      }
      self.poll_reader();
      // Keep the sink fed whatever the play state (a paused sink holds its
      // queue), so play starts with audio ready and a seek's preroll is
      // decoded while the picture is still held.
      if !self.resume_pending {
        if let Some(audio) = self.audio.as_mut() {
          let reader = &self.reader;
          audio.feed(|| reader.next_audio());
        }
      }
      // The sink learns of a stop before the park below, not after it.
      self.sync_releasing();
      // Nothing to do but wait for a command: block instead of spinning,
      // looking at the reader and the surface now and then. (Ending the
      // stream also parks here, until a seek restarts it.)
      if self.idle() {
        match self.rx.recv_timeout(IDLE_POLL) {
          Ok(cmd) => self.pending.push_back(cmd),
          Err(RecvTimeoutError::Timeout) => {}
          Err(RecvTimeoutError::Disconnected) => return,
        }
      }
      while let Ok(cmd) = self.rx.try_recv() {
        self.pending.push_back(cmd);
      }
      while let Some(cmd) = self.pending.pop_front() {
        if !self.handle(cmd) {
          return;
        }
      }
      self.sync_releasing();
      if self.idle() {
        continue;
      }
      self.feed();
      if self.buffering.active() {
        // Releases hold; the presenter keeps what it decoded. Pace the pass.
        wait_for_release(&self.rx, &mut self.pending, OUTPUT_WAIT.as_nanos() as i64);
      } else {
        self.present();
      }
    }
  }

  fn idle(&self) -> bool {
    (!self.playing && !self.show_one) || self.output_eos || self.failed
  }

  // Tell the presenter when releases start or stop: playing, and neither
  // buffering, failed nor at the end.
  fn sync_releasing(&mut self) {
    let releasing = self.playing && !self.buffering.active() && !self.failed && !self.output_eos;
    if releasing != self.releasing {
      self.releasing = releasing;
      self.presenter.set_playing(releasing);
    }
  }

  /// Apply one command; false on Close.
  fn handle(&mut self, cmd: Command) -> bool {
    match cmd {
      Command::Play => {
        if !self.playing {
          self.playing = true;
          // Resume anchors on the next frame: content continues from where
          // it paused, at the wall time it resumes, and the sound with it.
          self.reset_clock();
          self.shared.set_playing(true);
        }
      }
      Command::Pause => {
        self.playing = false;
        self.stop_audio();
        self.shared.set_playing(false);
      }
      Command::Seek(target_us) => self.seek(target_us),
      Command::Close => return false,
    }
    true
  }

  fn seek(&mut self, target_us: i64) {
    if let Err(e) = self.presenter.flush() {
      log::warn!("[forge::video] seek: flush: {e}");
      return;
    }
    // The surface (or the sink) keeps its last picture across the flush, so
    // the old picture holds until the frame at the target is released. The
    // reader empties its queues now and reports where the epoch resumes
    // (the target, or the keyframe after it) once its seek has landed;
    // until then the target stands in for both the skip and the audio
    // discard.
    self.epoch = self.reader.seek(target_us);
    self.resume_pending = true;
    self.skip_until = Some(target_us);
    self.stop_audio();
    if let Some(audio) = self.audio.as_mut() {
      audio.seek(target_us);
    }
    self.input_eos = false;
    self.output_eos = false;
    self.failed = false;
    self.input_stalled_since = None;
    self.shared.set_finished(false);
    self.show_one = !self.playing;
    self.reset_clock();
  }

  // Take the reader's state: the epoch's resume position once known, a
  // failure, and the supply the buffering rule judges.
  fn poll_reader(&mut self) {
    let status = self.reader.status();
    if status.epoch == self.epoch {
      if self.resume_pending {
        if let Some(resume_us) = status.resume_us {
          self.resume_pending = false;
          self.skip_until = Some(resume_us);
          if let Some(audio) = self.audio.as_mut() {
            audio.seek(resume_us);
          }
        }
      }
      if let Some(error) = status.error {
        if !self.failed {
          self.fail(error);
        }
      }
    }
    let supply = Supply {
      lead_us: status.lead_us,
      ended: status.ended,
      failed: self.failed,
      capped: status.capped,
      audio: self
        .audio
        .as_ref()
        .map(|audio| AudioSupply { queued_packets: status.audio_queued, sink_us: audio.queued_us() }),
    };
    match self.buffering.update(self.playing && !self.failed && !self.output_eos, &supply) {
      Some(true) => {
        // The sink pauses while it still holds audio, so its position
        // stays truthful for the resume.
        self.stop_audio();
        self.shared.set_buffering(true);
      }
      Some(false) => {
        self.shared.set_buffering(false);
        self.reset_clock();
      }
      None => {}
    }
  }

  // Forget the anchor and the audio lead with it (play, resume, seek,
  // stall): the next frame anchors the clock afresh.
  fn reset_clock(&mut self) {
    self.anchor.reset();
    self.sync.reset();
  }

  fn stop_audio(&mut self) {
    if self.audio_running {
      if let Some(audio) = self.audio.as_mut() {
        audio.set_playing(false);
      }
      self.audio_running = false;
    }
  }

  // The stream failed: playback stops where it is (the last frame stays
  // up), the failure is published, and `finished` stays as it was. A seek
  // retries the source.
  fn fail(&mut self, error: StreamError) {
    log::warn!("[forge::video] {error}");
    self.failed = true;
    self.playing = false;
    self.stop_audio();
    self.shared.set_playing(false);
    self.shared.set_error(error);
  }

  // The platform took the surface: nothing can be shown any more.
  fn lose(&mut self) {
    if self.output_eos {
      return;
    }
    log::info!("[forge::video] surface lost, ending playback");
    self.stop_audio();
    self.reader.close();
    self.finish();
  }

  /// Feed coded frames while the presenter takes them and the reader has
  /// them.
  fn feed(&mut self) {
    while !self.input_eos {
      match self.presenter.feed(&self.reader) {
        Ok(Feed::Took) => self.input_stalled_since = None,
        Ok(Feed::Full) | Ok(Feed::Waiting) => return,
        Ok(Feed::Refused { packet_waiting }) => {
          self.note_input_stall(packet_waiting);
          return;
        }
        Ok(Feed::End) => self.input_eos = true,
        Err(e) => {
          log::warn!("[forge::video] feed: {e}");
          return;
        }
      }
    }
  }

  // The presenter refused input. Counted only while frames are being
  // released (paused or buffering, the outputs are held and the inputs back
  // up by design); a refusal that lasts means a wedged codec.
  fn note_input_stall(&mut self, packet_waiting: bool) {
    if !packet_waiting || !self.playing || self.buffering.active() || self.show_one {
      self.input_stalled_since = None;
      return;
    }
    let since = *self.input_stalled_since.get_or_insert_with(Instant::now);
    if since.elapsed() >= INPUT_STALL {
      self.fail(StreamError::decode(format!("decoder accepted no input for {} ms", INPUT_STALL.as_millis())));
    }
  }

  /// Take one decoded picture, if any is ready within OUTPUT_WAIT, and
  /// release it against the clock (or skip it, after a seek or when late).
  fn present(&mut self) {
    let picture = match self.presenter.next(OUTPUT_WAIT) {
      Ok(Some(picture)) => picture,
      Ok(None) => return,
      Err(e) => {
        self.fail(e);
        return;
      }
    };
    let pts_us = picture.pts_us;
    let skip = picture.empty || self.skip_until.is_some_and(|target| pts_us < target);
    if skip {
      if let Err(e) = self.presenter.discard() {
        log::warn!("[forge::video] discard: {e}");
      }
    } else {
      self.skip_until = None;
      let result = if self.show_one {
        // A step while paused: the picture goes out for now.
        self.show_one = false;
        let now_ns = self.clock.now();
        self.presenter.release_at(self.presenter.snap(now_ns))
      } else {
        // A stepped clock is never corrected by the sound (the sink is a
        // real-time device that cannot clock a virtual timeline) and never
        // waited against (the consumer steps it, and holds every frame
        // pushed ahead until its time).
        if !self.audio_running {
          self.audio_running = true;
          start_audio(&mut self.audio, &mut self.anchor, &self.clock, &self.rx, &mut self.pending);
        }
        let now_ns = self.clock.now();
        if !self.clock.stepped {
          correct_clock(&self.audio, &mut self.anchor, &mut self.sync, now_ns);
        }
        let release_ns = self.presenter.snap(self.anchor.due_ns(pts_us, now_ns));
        let lead_ns = if self.clock.stepped { i64::MAX } else { self.presenter.lead_ns() };
        match classify(release_ns - now_ns, lead_ns) {
          Release::Wait(wait_ns) => {
            wait_for_release(&self.rx, &mut self.pending, wait_ns);
            self.presenter.release_at(release_ns)
          }
          Release::AtTime => self.presenter.release_at(release_ns),
          Release::Drop => {
            // Dropped, not shown; the position and the reader's head still
            // move past it, as for a released frame.
            self.dropped += 1;
            self.presenter.discard()
          }
          Release::Reanchor => {
            let late_ms = (now_ns - release_ns) / 1_000_000;
            log::info!("[forge::video] frame at {pts_us}us released {late_ms} ms late: clock re-anchored");
            self.anchor.reset();
            self.sync.reset();
            let now_ns = self.clock.now();
            let release_ns = self.presenter.snap(self.anchor.due_ns(pts_us, now_ns));
            self.presenter.release_at(release_ns)
          }
        }
      };
      match result {
        Ok(()) => {
          self.shared.set_position_us(pts_us);
          self.reader.released(pts_us);
        }
        Err(e) => log::warn!("[forge::video] release: {e}"),
      }
    }
    if picture.eos {
      self.finish();
    }
  }

  fn finish(&mut self) {
    self.output_eos = true;
    self.shared.set_finished(true);
    if self.buffering.active() {
      self.shared.set_buffering(false);
    }
    if self.dropped > 0 {
      log::info!("[forge::video] playback dropped {} late frames", self.dropped);
    }
  }
}

// The first frame after play, a seek or buffering is in hand, not yet
// released: start the sink, and once it consumes (its position moves)
// anchor the picture on the sound's content time, which is net of the
// output latency, so the two start together however long the device takes
// to resume. With nothing queued, no consumption within
// AUDIO_START_TIMEOUT, or a command arriving (queued for the next
// iteration), the frame anchors on itself instead and the sync corrects
// what is left. Against a stepped clock the track only starts: the anchor
// is the timeline's, not the sound's. (A free function: the caller holds
// the presenter's picture, so only these fields may be borrowed.)
fn start_audio(
  audio: &mut Option<AudioTrack>,
  anchor: &mut Anchor,
  clock: &Clock,
  rx: &Receiver<Command>,
  pending: &mut VecDeque<Command>,
) {
  let Some(audio) = audio.as_mut() else {
    return;
  };
  let from_us = audio.sink_position_us();
  audio.set_playing(true);
  if clock.stepped || audio.queued_us() == 0 {
    return;
  }
  let deadline = Instant::now() + AUDIO_START_TIMEOUT;
  while audio.sink_position_us() <= from_us {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
      let waited_ms = AUDIO_START_TIMEOUT.as_millis();
      log::info!("[forge::video] sink did not start within {waited_ms} ms: picture anchored on itself");
      return;
    }
    match rx.recv_timeout(left.min(AUDIO_START_POLL)) {
      Ok(cmd) => {
        pending.push_back(cmd);
        return;
      }
      Err(RecvTimeoutError::Timeout) => {}
      Err(RecvTimeoutError::Disconnected) => {
        pending.push_back(Command::Close);
        return;
      }
    }
  }
  if let Some(content_us) = audio.content_time_us() {
    anchor.set(clock.now(), content_us);
  }
}

// Let the audio clock correct the anchor before a frame is scheduled: when
// the sound's lead over the anchor, smoothed over frames, crosses the
// threshold, the anchor moves to it. Nothing happens before the first
// frame has anchored or before audio has started. (A free function, like
// start_audio: the caller holds the presenter's picture.)
fn correct_clock(audio: &Option<AudioTrack>, anchor: &mut Anchor, sync: &mut AudioSync, now_ns: i64) {
  let Some(audio) = audio.as_ref() else {
    return;
  };
  let (Some(audio_us), Some(expected_us)) = (audio.content_time_us(), anchor.content_at(now_ns)) else {
    return;
  };
  if let Some(shift_us) = sync.observe(audio_us - expected_us) {
    log::info!("[forge::video] audio clock lead {shift_us}us: anchor moved");
    anchor.shift(shift_us);
  }
}

// Sleep until the frame's release lead, or until a command arrives (queued
// for the next iteration; the frame in hand is still released at its time,
// which is at most one lead away).
fn wait_for_release(rx: &Receiver<Command>, pending: &mut VecDeque<Command>, wait_ns: i64) {
  let deadline = Instant::now() + Duration::from_nanos(wait_ns.max(0) as u64);
  let left = deadline.saturating_duration_since(Instant::now());
  if left.is_zero() {
    return;
  }
  match rx.recv_timeout(left) {
    Ok(cmd) => pending.push_back(cmd),
    Err(RecvTimeoutError::Timeout) => {}
    Err(RecvTimeoutError::Disconnected) => pending.push_back(Command::Close),
  }
}
