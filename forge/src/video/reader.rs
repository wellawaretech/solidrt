// The reader thread: one per streaming player. It owns the byte source and
// the demuxer, and fills a video and an audio packet queue in container
// order, ahead of the player by a bounded lead. The player's own thread
// never touches the network: it takes packets from the queues without
// blocking, and a source that stalls shows up as an empty queue, which the
// transport turns into buffering (transport.rs). Commands to the reader
// (a seek, a track change, close) interrupt a blocked read through the
// source's handle, so pause, seek and close stay prompt during a stall
// (okf/plans/video-streaming.md).
//
// Epochs have one owner, the reader's state, and do not care what bumped
// them: a seek here, a discontinuity later. The consumer bumps the epoch
// and clears the queues at once; the thread learns which epoch it serves
// when it takes the command, and a packet from before that never lands in
// the new epoch. The epoch's resume position (the seek target, or the first
// keyframe after it) is published when the demuxer has found it, and the
// player skips video and discards audio up to that one point.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use tokio::sync::oneshot;

use super::{AudioPacket, Demuxer, ErrorKind, MediaInfo, Packet, StreamError, VideoAu, WebmDemuxer};
use crate::source::{ByteSource, Handle};

// How far past the player's head the reader demuxes before it waits: the
// playback buffer. Above the resume buffer plus the audio lookahead (the
// sink's share of it), with room for a GOP; see the ordering test.
pub const STREAM_READ_AHEAD_US: i64 = 6_000_000;
// The hard cap on queued packet bytes, so a high-bitrate stream cannot
// hold the read-ahead in memory on a small device: above the peak bitrate
// the players are validated for times the read-ahead.
pub const STREAM_MAX_BUFFER_BYTES: usize = 24 * 1024 * 1024;
// The peak bitrate the byte cap is sized for (a 4K VP9 stream), for the
// ordering test only: the cap is what binds.
pub const PEAK_BITRATE_BPS: usize = 20_000_000;

/// What a queue answers: a packet, nothing yet (the source has not
/// delivered it: buffering, or an epoch whose seek is still in flight), or
/// the end of the stream with the queue drained. A failure is not an end:
/// it is read from [`Reader::status`], so a player never finishes on one.
pub enum Next<T> {
  Packet(T),
  Waiting,
  End,
}

/// The reader's published state, one snapshot per read.
#[derive(Clone, Debug)]
pub struct Status {
  /// The epoch the queues hold; bumped by every seek.
  pub epoch: u64,
  /// Where this epoch's playback resumes, once the demuxer has found the
  /// keyframe (the seek target, or that keyframe when it comes later); None
  /// while the seek is in flight.
  pub resume_us: Option<i64>,
  /// How far the newest demuxed packet runs ahead of the player's head.
  pub lead_us: i64,
  /// The byte cap binds: the reader waits with the lead short of the
  /// read-ahead, so a player must not wait for more.
  pub capped: bool,
  /// The demuxer reached the end of the stream (the queues may still hold
  /// its tail).
  pub ended: bool,
  /// Audio packets queued.
  pub audio_queued: usize,
  /// The failure that stopped the reader, until a seek retries.
  pub error: Option<StreamError>,
}

/// Opens the bytes on the reader thread: a file, or a URL whose request is
/// then in flight.
pub type Opener = Box<dyn FnOnce() -> io::Result<ByteSource> + Send>;

enum Command {
  Seek(i64),
  Tracks { video: bool, audio: bool },
}

struct State {
  epoch: u64,
  resume_us: Option<i64>,
  // The seek target until the epoch's first released frame, then the last
  // released pts: the lead is measured from here. Measured from the last
  // released pts alone, a forward seek past the read-ahead would park the
  // reader for good.
  head_us: i64,
  newest_us: i64,
  video: VecDeque<VideoAu>,
  audio: VecDeque<AudioPacket>,
  bytes: usize,
  ended: bool,
  error: Option<StreamError>,
  commands: VecDeque<Command>,
  closed: bool,
  // Which tracks are played: a packet of the other read before the thread
  // took the change is dropped at the push.
  play_video: bool,
  play_audio: bool,
  info: Option<MediaInfo>,
  // The byte source's handle, for interrupting a blocked read; set by the
  // thread as soon as the source is open, before its first blocking call.
  source: Option<Handle>,
}

impl State {
  fn lead_us(&self) -> i64 {
    self.newest_us - self.head_us
  }

  fn capped(&self) -> bool {
    self.bytes >= STREAM_MAX_BUFFER_BYTES
  }

  fn push(&mut self, packet: Packet) {
    match packet {
      Packet::Video(au) if self.play_video => {
        self.newest_us = self.newest_us.max(au.pts_us);
        self.bytes += au.data.len();
        self.video.push_back(au);
      }
      Packet::Audio(packet) if self.play_audio => {
        self.newest_us = self.newest_us.max(packet.pts_us);
        self.bytes += packet.data.len();
        self.audio.push_back(packet);
      }
      _ => {}
    }
  }

  fn clear(&mut self) {
    self.video.clear();
    self.audio.clear();
    self.bytes = 0;
  }
}

struct Shared {
  state: Mutex<State>,
  // Signalled on every change the thread may be parked on: room in the
  // queues, a command, close.
  wake: Condvar,
}

impl Shared {
  fn lock(&self) -> MutexGuard<'_, State> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }
}

/// The consumer's end of a reader thread: packet queues, seek, status.
/// Dropping it closes the source and joins the thread (prompt: a blocked
/// read is interrupted, a parked thread is woken).
pub struct Reader {
  shared: Arc<Shared>,
  thread: Option<JoinHandle<()>>,
}

/// A clonable way to close a reader from a thread that does not own it.
#[derive(Clone)]
pub struct ReaderHandle {
  shared: Arc<Shared>,
}

impl Reader {
  /// Start a reader over the bytes `open` yields. Returns at once with the
  /// reader and the answer to the open: the stream's facts once the header
  /// and the first keyframe have been read, or the failure. Dropping the
  /// receiver abandons the open: the thread stops as soon as it notices.
  pub fn open(open: Opener) -> (Reader, oneshot::Receiver<Result<MediaInfo, StreamError>>) {
    let shared = Arc::new(Shared {
      state: Mutex::new(State {
        epoch: 0,
        resume_us: None,
        head_us: 0,
        newest_us: 0,
        video: VecDeque::new(),
        audio: VecDeque::new(),
        bytes: 0,
        ended: false,
        error: None,
        commands: VecDeque::new(),
        closed: false,
        play_video: true,
        play_audio: true,
        info: None,
        source: None,
      }),
      wake: Condvar::new(),
    });
    let (answer, opened) = oneshot::channel();
    let worker = shared.clone();
    let thread = thread::Builder::new()
      .name("srt-video-reader".to_string())
      .spawn(move || run(worker, open, answer))
      .map_err(|e| log::warn!("[forge::video] spawn reader: {e}"))
      .ok();
    (Reader { shared, thread }, opened)
  }

  /// The stream's facts, once the open has answered.
  pub fn info(&self) -> Option<MediaInfo> {
    self.shared.lock().info.clone()
  }

  pub fn handle(&self) -> ReaderHandle {
    ReaderHandle { shared: self.shared.clone() }
  }

  /// Which tracks are played: the other's packets are dropped at the
  /// source and whatever it queued goes now.
  pub fn set_tracks(&self, video: bool, audio: bool) {
    let mut state = self.shared.lock();
    state.play_video = video;
    state.play_audio = audio;
    if !video {
      state.bytes -= state.video.iter().map(|au| au.data.len()).sum::<usize>();
      state.video.clear();
    }
    if !audio {
      state.bytes -= state.audio.iter().map(|p| p.data.len()).sum::<usize>();
      state.audio.clear();
    }
    state.commands.push_back(Command::Tracks { video, audio });
    self.shared.wake.notify_all();
  }

  /// Start a new epoch at `target_us`: the queues empty now, and fill again
  /// from the keyframe at or before the target once the thread has taken
  /// the seek (interrupting a blocked read). Pending seeks collapse to this
  /// one. Returns the new epoch; on a source that cannot seek nothing
  /// happens and the current epoch is returned.
  pub fn seek(&self, target_us: i64) -> u64 {
    let mut state = self.shared.lock();
    if !state.info.as_ref().is_some_and(|info| info.seekable) {
      return state.epoch;
    }
    state.epoch += 1;
    state.clear();
    state.resume_us = None;
    state.head_us = target_us;
    state.newest_us = target_us;
    state.ended = false;
    state.error = None;
    state.commands.retain(|c| !matches!(c, Command::Seek(_)));
    state.commands.push_back(Command::Seek(target_us));
    let epoch = state.epoch;
    if let Some(source) = &state.source {
      source.interrupt();
    }
    self.shared.wake.notify_all();
    epoch
  }

  /// Whether `next_video` would hand a packet out now (a codec input buffer
  /// must not be dequeued for nothing).
  pub fn video_ready(&self) -> Next<()> {
    let state = self.shared.lock();
    if !state.video.is_empty() {
      Next::Packet(())
    } else if state.ended {
      Next::End
    } else {
      Next::Waiting
    }
  }

  pub fn next_video(&self) -> Next<VideoAu> {
    let mut state = self.shared.lock();
    match state.video.pop_front() {
      Some(au) => {
        state.bytes -= au.data.len();
        self.shared.wake.notify_all();
        Next::Packet(au)
      }
      None if state.ended => Next::End,
      None => Next::Waiting,
    }
  }

  pub fn next_audio(&self) -> Next<AudioPacket> {
    let mut state = self.shared.lock();
    match state.audio.pop_front() {
      Some(packet) => {
        state.bytes -= packet.data.len();
        self.shared.wake.notify_all();
        Next::Packet(packet)
      }
      None if state.ended => Next::End,
      None => Next::Waiting,
    }
  }

  /// A frame at `pts_us` was released: the head moves there.
  pub fn released(&self, pts_us: i64) {
    let mut state = self.shared.lock();
    state.head_us = pts_us;
    self.shared.wake.notify_all();
  }

  pub fn status(&self) -> Status {
    let state = self.shared.lock();
    Status {
      epoch: state.epoch,
      resume_us: state.resume_us,
      lead_us: state.lead_us(),
      capped: state.capped(),
      ended: state.ended,
      audio_queued: state.audio.len(),
      error: state.error.clone(),
    }
  }

  pub fn close(&self) {
    self.handle().close();
  }
}

impl Drop for Reader {
  fn drop(&mut self) {
    self.close();
    if let Some(thread) = self.thread.take() {
      if thread.join().is_err() {
        log::warn!("[forge::video] reader thread panicked");
      }
    }
  }
}

impl ReaderHandle {
  /// Stop the thread: a blocked read is interrupted and the source
  /// cancelled, a parked thread woken. Returns at once.
  pub fn close(&self) {
    let mut state = self.shared.lock();
    state.closed = true;
    if let Some(source) = state.source.take() {
      source.close();
    }
    self.shared.wake.notify_all();
  }
}

// --- The thread ---

fn run(shared: Arc<Shared>, open: Opener, answer: oneshot::Sender<Result<MediaInfo, StreamError>>) {
  let mut demux = match open_stream(&shared, open) {
    Ok(demux) => demux,
    Err(e) => {
      let _ = answer.send(Err(e));
      return;
    }
  };
  let info = demux.info().clone();
  {
    let mut state = shared.lock();
    state.info = Some(info.clone());
    state.resume_us = Some(info.start_us);
    state.head_us = info.start_us;
    state.newest_us = info.start_us;
  }
  if answer.send(Ok(info)).is_err() {
    // Nobody waits for the open (aborted, or its owner is gone).
    return;
  }

  // The epoch the packets read next belong to; a packet read for an older
  // one is dropped at the push.
  let mut serving = 0;
  loop {
    // Park while a bound binds, the stream is over or failed, until a
    // command or room arrives.
    let command = {
      let mut state = shared.lock();
      loop {
        if state.closed {
          return;
        }
        if let Some(command) = state.commands.pop_front() {
          // The interrupt that announced a command has done its job. Left
          // pending (the thread was parked, not reading), it would fail the
          // command's own first read, such as a seek's jump to the tail
          // Cues. Commands are queued under this lock, so a newer one's
          // interrupt comes after this; and every command below continues
          // the loop, so a seek queued behind this one is taken before the
          // next read, not left to wait on a stalled body.
          if let Some(source) = &state.source {
            source.clear_interrupt();
          }
          break Some(command);
        }
        let parked = state.ended || state.error.is_some() || state.lead_us() >= STREAM_READ_AHEAD_US || state.capped();
        if !parked {
          break None;
        }
        state = shared.wake.wait(state).unwrap_or_else(PoisonError::into_inner);
      }
    };
    match command {
      None => {}
      Some(Command::Tracks { video, audio }) => {
        demux.set_tracks(video, audio);
        continue;
      }
      Some(Command::Seek(target_us)) => {
        serving = shared.lock().epoch;
        match demux.seek(target_us) {
          Ok(resume_us) => {
            let mut state = shared.lock();
            if state.epoch == serving {
              state.resume_us = Some(resume_us);
            }
          }
          // A newer command interrupted the seek: the loop takes it.
          Err(e) if e.kind == ErrorKind::Interrupted => {}
          Err(e) => {
            log::warn!("[forge::video] seek: {e}");
            let mut state = shared.lock();
            if state.epoch == serving {
              state.error = Some(e);
            }
          }
        }
        continue;
      }
    }
    match demux.next_packet() {
      Ok(Some(packet)) => {
        let mut state = shared.lock();
        if state.epoch == serving {
          state.push(packet);
        }
      }
      Ok(None) => {
        let mut state = shared.lock();
        if state.epoch == serving {
          state.ended = true;
        }
      }
      // A command is waiting: the loop takes it, and the read that was
      // interrupted is redone by the seek it came with.
      Err(e) if e.kind == ErrorKind::Interrupted => {}
      Err(e) => {
        log::warn!("[forge::video] {e}");
        let mut state = shared.lock();
        if state.epoch == serving {
          state.error = Some(e);
        }
      }
    }
  }
}

// Open the bytes, publish the handle that interrupts them, then read the
// header: a close during the open ends the wait for the source's answer.
fn open_stream(shared: &Shared, open: Opener) -> Result<WebmDemuxer, StreamError> {
  let source = open().map_err(|e| StreamError::network(e.to_string()))?;
  {
    let mut state = shared.lock();
    if state.closed {
      return Err(StreamError::network("closed while opening"));
    }
    state.source = source.handle();
  }
  let facts = source.facts().map_err(|e| StreamError::network(e.to_string()))?;
  WebmDemuxer::open(source.into_reader(), facts)
}
