// The plane player (Android): AMediaCodec in SURFACE mode, decoding straight
// into a platform surface the compositor presents. No frame ever reaches
// Rust - `releaseOutputBufferAtTime` hands each decoded buffer to the
// surface with the system time it is due at, and SurfaceFlinger latches it
// on that vsync. Our frame loop is not involved at all
// (okf/plans/android-video-punch-through.md).
//
// One worker thread owns the demuxer, the codec and the transport anchor,
// drains commands between frames, and publishes position/playing/finished.
// It never blocks the caller: the codec is constructed on the worker (with
// the clip-handover retry the texture path also uses), and the caller reads
// state from atomics.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ndk::media::media_codec::{
  DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;
use ndk::native_window::NativeWindow;

use super::mediacodec::{create_with_retry, FLAG_END_OF_STREAM, MIME_VP9};
use super::transport::{classify, monotonic_ns, Anchor, Command, Controls, Release, Shared, VsyncGrid, RELEASE_LEAD_NS};
use super::{Demuxer, MediaInfo};

// How long one output dequeue waits when the codec has nothing ready: the
// worker's idle granularity while playing (commands are drained between
// dequeues, so this also bounds command latency).
const OUTPUT_WAIT: Duration = Duration::from_millis(10);

/// A player presenting on a plane: opened on a demuxer and the plane's
/// surface, driven through play/pause/seek, read through position/finished.
/// Dropping it ends the worker and releases the codec (and with it the
/// surface) before returning, so the plane can be torn down right after.
pub struct PlanePlayer {
  controls: Controls,
  info: MediaInfo,
  worker: Option<JoinHandle<()>>,
}

impl PlanePlayer {
  /// Start decoding `demux` into `window`, snapping release times onto
  /// `vsync` when the display's grid is known. Playback starts paused; the
  /// first frame is shown on `play`. A codec that cannot be created ends the
  /// stream (`finished` goes true, the reason is logged), like the texture
  /// path.
  pub fn open(demux: Box<dyn Demuxer>, window: NativeWindow, vsync: Option<VsyncGrid>) -> Result<PlanePlayer, String> {
    let info = demux.info().clone();
    let (width, height) = (info.width, info.height);
    let (controls, rx, shared) = Controls::new();
    let worker = thread::Builder::new()
      .name("srt-video-plane".to_string())
      .spawn(move || {
        let codec = match create_with_retry(|| open_codec(&window, width, height)) {
          Ok(codec) => codec,
          Err(e) => {
            log::warn!("[forge::video] plane decoder: {e}");
            shared.set_finished(true);
            return;
          }
        };
        Worker {
          codec,
          demux,
          rx,
          shared,
          anchor: Anchor::new(),
          vsync,
          playing: false,
          input_eos: false,
          output_eos: false,
          skip_until: None,
          show_one: false,
          pending: VecDeque::new(),
          dropped: 0,
        }
        .run();
        // The codec drops here (stopped and deleted), releasing the surface.
      })
      .map_err(|e| format!("spawn video plane worker: {e}"))?;
    Ok(PlanePlayer { controls, info, worker: Some(worker) })
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
  /// is shown; while playing playback continues from it.
  pub fn seek(&self, target_us: i64) {
    self.controls.seek(target_us);
  }

  pub fn playing(&self) -> bool {
    self.controls.playing()
  }

  /// Presentation time of the last frame released to the surface.
  pub fn position_us(&self) -> i64 {
    self.controls.position_us()
  }

  /// Whether the stream's last frame has been released.
  pub fn finished(&self) -> bool {
    self.controls.finished()
  }
}

impl Drop for PlanePlayer {
  fn drop(&mut self) {
    self.controls.close();
    if let Some(worker) = self.worker.take() {
      if worker.join().is_err() {
        log::warn!("[forge::video] plane worker panicked");
      }
    }
  }
}

fn open_codec(window: &NativeWindow, width: u32, height: u32) -> Result<MediaCodec, String> {
  let codec =
    MediaCodec::from_decoder_type(MIME_VP9).ok_or_else(|| format!("no {MIME_VP9} decoder on this device"))?;
  let mut format = MediaFormat::new();
  format.set_str("mime", MIME_VP9);
  format.set_i32("width", width as i32);
  format.set_i32("height", height as i32);
  codec
    .configure(&format, Some(window), MediaCodecDirection::Decoder)
    .map_err(|e| format!("configure {MIME_VP9} decoder on a surface: {e:?}"))?;
  codec.start().map_err(|e| format!("start {MIME_VP9} decoder: {e:?}"))?;
  Ok(codec)
}

struct Worker {
  codec: MediaCodec,
  demux: Box<dyn Demuxer>,
  rx: Receiver<Command>,
  shared: Arc<Shared>,
  anchor: Anchor,
  // The display's vsync grid, when the plane's owner could read it.
  vsync: Option<VsyncGrid>,
  playing: bool,
  // End of stream has been queued to the codec / has come out of it.
  input_eos: bool,
  output_eos: bool,
  // After a seek: frames before this pts are decoded but not shown (the
  // decode started at the keyframe before the target).
  skip_until: Option<i64>,
  // After a seek while paused: show the first frame at the target, once.
  show_one: bool,
  // Commands received while waiting for a frame's release time, handled at
  // the top of the next iteration.
  pending: VecDeque<Command>,
  dropped: u64,
}

impl Worker {
  fn run(mut self) {
    loop {
      // Nothing to do but wait for a command: block instead of spinning.
      // (Ending the stream also parks here, until a seek restarts it.)
      if self.idle() {
        match self.rx.recv() {
          Ok(cmd) => self.pending.push_back(cmd),
          Err(_) => return,
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
      if self.idle() {
        continue;
      }
      self.feed();
      self.present();
    }
  }

  fn idle(&self) -> bool {
    (!self.playing && !self.show_one) || self.output_eos
  }

  /// Apply one command; false on Close.
  fn handle(&mut self, cmd: Command) -> bool {
    match cmd {
      Command::Play => {
        if !self.playing {
          self.playing = true;
          // Resume anchors on the next frame: content continues from where
          // it paused, at the wall time it resumes.
          self.anchor.reset();
          self.shared.set_playing(true);
        }
      }
      Command::Pause => {
        self.playing = false;
        self.shared.set_playing(false);
      }
      Command::Seek(target_us) => self.seek(target_us),
      Command::Close => return false,
    }
    true
  }

  fn seek(&mut self, target_us: i64) {
    if let Err(e) = self.codec.flush() {
      log::warn!("[forge::video] seek: flush: {e:?}");
      return;
    }
    // The surface keeps its last latched frame across the flush, so the old
    // picture holds until the frame at the target is released.
    match self.demux.seek(target_us) {
      Ok(()) => self.skip_until = Some(target_us),
      Err(e) => {
        // Not seekable (a live source): the flushed codec resumes from the
        // next frame the source delivers.
        log::warn!("[forge::video] seek: {e}");
        self.skip_until = None;
      }
    }
    self.input_eos = false;
    self.output_eos = false;
    self.shared.set_finished(false);
    self.show_one = !self.playing;
    self.anchor.reset();
  }

  /// Queue coded frames while the codec has free input buffers.
  fn feed(&mut self) {
    while !self.input_eos {
      let mut buf = match self.codec.dequeue_input_buffer(Duration::ZERO) {
        Ok(DequeuedInputBufferResult::Buffer(buf)) => buf,
        Ok(DequeuedInputBufferResult::TryAgainLater) => return,
        Err(e) => {
          log::warn!("[forge::video] dequeue input: {e:?}");
          return;
        }
      };
      // The dequeued buffer must go back queued; skip frames that do not fit
      // (fail-soft) rather than lose the buffer.
      let next = loop {
        match self.demux.next_video() {
          Ok(Some(au)) => {
            let target = buf.buffer_mut();
            if target.len() < au.data.len() {
              log::warn!(
                "[forge::video] skipping frame at {}us: input buffer too small ({} < {})",
                au.pts_us,
                target.len(),
                au.data.len()
              );
              continue;
            }
            break Some(au);
          }
          Ok(None) => break None,
          Err(e) => {
            // A read error ends the stream like end of file would.
            log::warn!("[forge::video] {e}");
            break None;
          }
        }
      };
      let result = match next {
        Some(au) => {
          let target = buf.buffer_mut();
          for (dst, src) in target.iter_mut().zip(&au.data) {
            dst.write(*src);
          }
          self.codec.queue_input_buffer(buf, 0, au.data.len(), au.pts_us.max(0) as u64, 0)
        }
        None => {
          self.input_eos = true;
          self.codec.queue_input_buffer(buf, 0, 0, 0, FLAG_END_OF_STREAM)
        }
      };
      if let Err(e) = result {
        log::warn!("[forge::video] queue input: {e:?}");
        return;
      }
    }
  }

  /// Take one decoded buffer, if any is ready within OUTPUT_WAIT, and
  /// release it against the clock (or skip it, after a seek or when late).
  fn present(&mut self) {
    let buf = match self.codec.dequeue_output_buffer(OUTPUT_WAIT) {
      Ok(DequeuedOutputBufferInfoResult::Buffer(buf)) => buf,
      Ok(_) => return,
      Err(e) => {
        // A codec error mid-stream (the surface went away beneath it) ends
        // the stream; the owner learns the cause from the plane itself.
        log::warn!("[forge::video] dequeue output: {e:?}");
        self.finish();
        return;
      }
    };
    let info = *buf.info();
    let eos = info.flags() & FLAG_END_OF_STREAM != 0;
    let pts_us = info.presentation_time_us();
    let skip = info.size() == 0 || self.skip_until.is_some_and(|target| pts_us < target);
    if skip {
      if let Err(e) = self.codec.release_output_buffer(buf, false) {
        log::warn!("[forge::video] release output: {e:?}");
      }
    } else {
      self.skip_until = None;
      let result = if self.show_one {
        self.show_one = false;
        self.codec.release_output_buffer(buf, true)
      } else {
        let now_ns = monotonic_ns();
        let due_ns = release_ns(&mut self.anchor, &self.vsync, pts_us, now_ns);
        let lead_ns = self.vsync.as_ref().map_or(RELEASE_LEAD_NS, VsyncGrid::lead_ns);
        let cls = classify(due_ns - now_ns, lead_ns);
        let vs = self.vsync.as_ref().and_then(|g| (g.sample_ns)()).unwrap_or(0);
        let r = match cls {
          Release::Wait(wait_ns) => {
            wait_for_release(&self.rx, &mut self.pending, wait_ns);
            self.codec.release_output_buffer_at_time(buf, due_ns)
          }
          Release::AtTime => self.codec.release_output_buffer_at_time(buf, due_ns),
          Release::Drop => {
            self.dropped += 1;
            self.codec.release_output_buffer(buf, false)
          }
          Release::Reanchor => {
            self.anchor.reset();
            let due_ns = release_ns(&mut self.anchor, &self.vsync, pts_us, monotonic_ns());
            self.codec.release_output_buffer_at_time(buf, due_ns)
          }
        };
        // PROBE (temporary): one line per frame for the tablet cadence hunt.
        log::warn!(
          "PROBE pts={pts_us} out={now_ns} due={due_ns} rel={} vs={vs} lead={lead_ns} cls={cls:?}",
          monotonic_ns()
        );
        r
      };
      match result {
        Ok(()) => self.shared.set_position_us(pts_us),
        Err(e) => log::warn!("[forge::video] release output: {e:?}"),
      }
    }
    if eos {
      self.finish();
    }
  }

  fn finish(&mut self) {
    self.output_eos = true;
    self.shared.set_finished(true);
    if self.dropped > 0 {
      log::info!("[forge::video] plane playback dropped {} late frames", self.dropped);
    }
  }
}

// The system time to release the frame at `pts_us` for: its due time on
// the anchor, snapped onto the vsync grid when one is known. (A free
// function: the caller holds the codec's output buffer, so only these two
// fields may be borrowed.)
fn release_ns(anchor: &mut Anchor, vsync: &Option<VsyncGrid>, pts_us: i64, now_ns: i64) -> i64 {
  let due_ns = anchor.due_ns(pts_us, now_ns);
  match vsync {
    Some(grid) => grid.snap(due_ns),
    None => due_ns,
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
