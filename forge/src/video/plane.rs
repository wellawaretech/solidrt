// The plane player (Android): AMediaCodec in SURFACE mode, decoding straight
// into a platform surface the compositor presents. No frame ever reaches
// Rust - `releaseOutputBufferAtTime` hands each decoded buffer to the
// surface with the system time it is due at, and SurfaceFlinger latches it
// on that vsync. Our frame loop is not involved at all
// (okf/plans/android-video-punch-through.md).
//
// This file is the surface presenter the shared worker (worker.rs) drives:
// the codec input step, one output dequeue held until the worker releases
// or discards it, and the release snapped onto the display's vsync grid.
// The presenter's host owns the codec AND the plane's surface, in that
// order, so dropping it on the worker releases the surface first and then
// removes the view; the worker's exit (`Shared::exited`) is what a
// successor that needs the platform's one plane waits for.

use std::any::Any;
use std::time::Duration;

use ndk::media::media_codec::{DequeuedOutputBufferInfoResult, MediaCodec, OutputBuffer};
use ndk::native_window::NativeWindow;

use super::mediacodec::{create_with_retry, feed_codec, open_codec, FLAG_END_OF_STREAM};
use super::reader::Reader;
use super::transport::{AudioSink, Clock, VsyncGrid, RELEASE_LEAD_NS};
use super::worker::{Feed, LostSampler, Picture, Player, Presenter, PresenterHost};
use super::StreamError;

/// Open a plane player: decode the stream `reader` delivers into `window`,
/// snapping release times onto `vsync` when the display's grid is known,
/// and its audio track (when the stream has one and the caller provides a
/// `sink` for it) into that sink. `plane` is the window's owner (the
/// platform's plane view), held by the player for the codec's lifetime and
/// dropped on the worker after the codec has released the surface, so its
/// drop may remove the view; it must be usable from that thread. Playback
/// starts paused. A codec that cannot be created ends the stream
/// (`finished` goes true, the reason is logged and published). `lost`
/// reports the surface gone.
pub fn open(
  reader: Reader,
  window: NativeWindow,
  plane: Box<dyn Any + Send>,
  vsync: Option<VsyncGrid>,
  sink: Option<Box<dyn AudioSink>>,
  lost: LostSampler,
) -> Result<Player, String> {
  let info = reader.info().ok_or("stream not opened")?;
  let (width, height) = (info.width, info.height);
  let make = Box::new(move || {
    let codec = create_with_retry(|| open_codec(Some(&window), width, height))?;
    Ok(Box::new(SurfaceHost { codec, _window: window, _plane: plane, vsync }) as Box<dyn PresenterHost>)
  });
  Player::open("srt-video-plane", reader, make, Clock::monotonic(), sink, lost)
}

struct SurfaceHost {
  // Field order is drop order: the codec (stopped and deleted, releasing
  // the surface) before the surface and its owner (whose drop removes the
  // view). Both are held for that alone.
  codec: MediaCodec,
  _window: NativeWindow,
  _plane: Box<dyn Any + Send>,
  // The display's vsync grid, when the plane's owner could read it.
  vsync: Option<VsyncGrid>,
}

impl PresenterHost for SurfaceHost {
  fn presenter(&mut self) -> Box<dyn Presenter + '_> {
    Box::new(SurfacePresenter { codec: &self.codec, vsync: &self.vsync, held: None })
  }
}

struct SurfacePresenter<'c> {
  codec: &'c MediaCodec,
  vsync: &'c Option<VsyncGrid>,
  // The output buffer made current by `next`, until it is released or
  // discarded.
  held: Option<OutputBuffer<'c>>,
}

impl<'c> SurfacePresenter<'c> {
  fn take_held(&mut self) -> Result<OutputBuffer<'c>, String> {
    self.held.take().ok_or_else(|| "no picture is current".to_string())
  }
}

impl Presenter for SurfacePresenter<'_> {
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError> {
    feed_codec(&self.codec, reader)
  }

  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError> {
    if self.held.is_some() {
      return Err(StreamError::decode("a picture is already current"));
    }
    let buf = match self.codec.dequeue_output_buffer(wait) {
      Ok(DequeuedOutputBufferInfoResult::Buffer(buf)) => buf,
      Ok(_) => return Ok(None),
      Err(e) => {
        // A codec error mid-stream (the surface went away beneath it) ends
        // the stream; the owner learns the cause from the plane itself.
        log::warn!("[forge::video] dequeue output: {e:?}");
        return Ok(Some(Picture { pts_us: 0, eos: true, empty: true }));
      }
    };
    let info = *buf.info();
    self.held = Some(buf);
    Ok(Some(Picture {
      pts_us: info.presentation_time_us(),
      eos: info.flags() & FLAG_END_OF_STREAM != 0,
      empty: info.size() == 0,
    }))
  }

  fn snap(&self, due_ns: i64) -> i64 {
    match self.vsync {
      Some(grid) => grid.snap(due_ns),
      None => due_ns,
    }
  }

  fn release_at(&mut self, release_ns: i64) -> Result<(), String> {
    let buf = self.take_held()?;
    self.codec.release_output_buffer_at_time(buf, release_ns).map_err(|e| format!("release output: {e:?}"))
  }

  fn discard(&mut self) -> Result<(), String> {
    let buf = self.take_held()?;
    self.codec.release_output_buffer(buf, false).map_err(|e| format!("release output: {e:?}"))
  }

  fn flush(&mut self) -> Result<(), String> {
    // A flush returns every buffer to the codec, the held one included
    // (ndk's OutputBuffer has no Drop of its own).
    self.held = None;
    self.codec.flush().map_err(|e| format!("flush: {e:?}"))
  }

  fn lead_ns(&self) -> i64 {
    self.vsync.as_ref().map_or(RELEASE_LEAD_NS, VsyncGrid::lead_ns)
  }
}
