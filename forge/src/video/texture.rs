// The texture player: a decoder's pictures as packed YUV frames, pushed
// to the compositor's latch with the time each is due
// (transport::FrameSink), from the shared worker (worker.rs). The
// compositor shows the newest due frame at its own cadence; nothing about
// the video runs on the caller's thread (okf/plans/video-texture-off-frame-
// loop.md). The presenter is the synchronous decoder over the sink - the
// vendored libvpx everywhere but Android, whose buffer-mode MediaCodec has
// its own presenter in mediacodec.rs, since its decode is asynchronous.

#[cfg(not(target_os = "android"))]
use std::collections::VecDeque;
#[cfg(not(target_os = "android"))]
use std::time::Duration;

#[cfg(not(target_os = "android"))]
use super::reader::Next;
use super::reader::Reader;
use super::transport::{AudioSink, Clock, FrameSink, RELEASE_LEAD_NS};
#[cfg(not(target_os = "android"))]
use super::worker::{Feed, Picture, Presenter};
use super::worker::{Player, PresenterFactory, PresenterHost};
#[cfg(not(target_os = "android"))]
use super::{StreamError, VideoDecoder, YuvFrame};

// Decoded frames the presenter holds ahead of the worker: two, since a
// 1080p frame is 3 MB and the sink queues ahead of it. The reader's queue
// is the real lookahead.
#[cfg(not(target_os = "android"))]
const PRESENTER_QUEUE: usize = 2;
// How far ahead of its due time a frame is pushed to the sink, in the
// sink's refresh periods. The compositor here is the UI thread's frame
// build, which starts one period before the present and takes the frames
// due up to half a period after it (the latch's lookahead); the build's
// content check has to see the frame at its start, so the push leads by
// the build period plus the lookahead, rounded up to whole periods. One
// more than the plane's lead, whose compositor takes a hand-over up to
// the vsync itself. Measured on a 60 Hz desktop with 50 fps content: at
// one period half the frames were latched behind the check
// (videoLateLatches), at two none.
const TEXTURE_LEAD_PERIODS: i64 = 2;

/// Open a texture player: decode the stream `reader` delivers and push its
/// frames into `sink`, scheduled on `clock` (the compositor's own, so the
/// due times and the sink's deadlines agree), with its audio track (when
/// the stream has one and the caller provides `audio`) into that sink.
/// Playback starts paused. A decoder that cannot be created ends the
/// stream (`finished` goes true, the reason is logged and published).
pub fn open(
  reader: Reader,
  sink: Box<dyn FrameSink>,
  clock: Clock,
  audio: Option<Box<dyn AudioSink>>,
) -> Result<Player, String> {
  let info = reader.info().ok_or("stream not opened")?;
  let (width, height) = (info.width, info.height);
  let make: PresenterFactory = Box::new(move || make_presenter(width, height, sink));
  Player::open("srt-video-texture", reader, make, clock, audio, Box::new(|| false))
}

#[cfg(not(target_os = "android"))]
fn make_presenter(width: u32, height: u32, sink: Box<dyn FrameSink>) -> Result<Box<dyn PresenterHost>, String> {
  let decoder = super::vpx::Vp9Decoder::new(width, height)?;
  Ok(Box::new(DecoderPresenter::new(decoder, sink)))
}

#[cfg(target_os = "android")]
fn make_presenter(width: u32, height: u32, sink: Box<dyn FrameSink>) -> Result<Box<dyn PresenterHost>, String> {
  let codec = super::mediacodec::create_with_retry(|| super::mediacodec::open_codec(None, width, height))?;
  Ok(Box::new(super::mediacodec::BufferPresenter::new(codec, width, height, sink)))
}

/// The lead a texture presenter releases by: `TEXTURE_LEAD_PERIODS`
/// compositor periods when the sink knows its period, the fallback
/// otherwise.
pub(crate) fn sink_lead_ns(sink: &dyn FrameSink) -> i64 {
  sink.period_ns().map_or(RELEASE_LEAD_NS, |period| period * TEXTURE_LEAD_PERIODS)
}

// A decoded picture in the presenter's queue, or the stream's end behind
// the last one.
#[cfg(not(target_os = "android"))]
enum Slot {
  Frame(YuvFrame),
  End { pts_us: i64 },
}

/// A synchronous decoder over a frame sink: `feed` decodes the one coded
/// frame it takes into a small queue, `next` pops it, `release_at` pushes
/// it to the sink with its due time. Owns everything it needs, so it is its
/// own host. Not on Android, whose texture decoder is asynchronous
/// (mediacodec.rs).
#[cfg(not(target_os = "android"))]
pub(crate) struct DecoderPresenter<D: VideoDecoder> {
  decoder: D,
  sink: Box<dyn FrameSink>,
  queue: VecDeque<Slot>,
  // The picture made current by `next`, with whether it is the stream's
  // last (the end goes to the sink when it is released or discarded).
  current: Option<(Option<YuvFrame>, bool)>,
  last_pts_us: i64,
}

#[cfg(not(target_os = "android"))]
impl<D: VideoDecoder> DecoderPresenter<D> {
  pub(crate) fn new(decoder: D, sink: Box<dyn FrameSink>) -> Self {
    DecoderPresenter { decoder, sink, queue: VecDeque::new(), current: None, last_pts_us: 0 }
  }

  fn queue_frames(&mut self, frames: Vec<YuvFrame>) {
    for frame in frames {
      self.last_pts_us = frame.pts_us;
      self.queue.push_back(Slot::Frame(frame));
    }
  }

  fn take_current(&mut self) -> Result<(Option<YuvFrame>, bool), String> {
    self.current.take().ok_or_else(|| "no picture is current".to_string())
  }
}

#[cfg(not(target_os = "android"))]
impl<D: VideoDecoder> Presenter for DecoderPresenter<D> {
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError> {
    if self.queue.len() >= PRESENTER_QUEUE {
      return Ok(Feed::Full);
    }
    match reader.next_video() {
      Next::Packet(au) => {
        match self.decoder.decode(&au) {
          Ok(frames) => self.queue_frames(frames),
          // Per-frame decode errors skip the frame (fail-soft playback:
          // one bad frame must not kill the stream).
          Err(e) => log::warn!("[forge::video] skipping frame at {}us: {e}", au.pts_us),
        }
        Ok(Feed::Took)
      }
      Next::Waiting => Ok(Feed::Waiting),
      Next::End => {
        match self.decoder.flush() {
          Ok(frames) => self.queue_frames(frames),
          Err(e) => log::warn!("[forge::video] {e}"),
        }
        self.queue.push_back(Slot::End { pts_us: self.last_pts_us });
        Ok(Feed::End)
      }
    }
  }

  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError> {
    if self.current.is_some() {
      return Err(StreamError::decode("a picture is already current"));
    }
    match self.queue.pop_front() {
      Some(Slot::Frame(frame)) => {
        let pts_us = frame.pts_us;
        self.current = Some((Some(frame), false));
        Ok(Some(Picture { pts_us, eos: false, empty: false }))
      }
      Some(Slot::End { pts_us }) => {
        self.current = Some((None, true));
        Ok(Some(Picture { pts_us, eos: true, empty: true }))
      }
      // Nothing decoded: the reader had nothing to feed. Pace the pass the
      // way a codec's output wait does.
      None => {
        std::thread::sleep(wait);
        Ok(None)
      }
    }
  }

  fn snap(&self, due_ns: i64) -> i64 {
    due_ns
  }

  fn release_at(&mut self, release_ns: i64) -> Result<(), String> {
    let (frame, eos) = self.take_current()?;
    if let Some(frame) = frame {
      self.sink.push(frame, release_ns);
    }
    if eos {
      self.sink.end();
    }
    Ok(())
  }

  fn discard(&mut self) -> Result<(), String> {
    let (_, eos) = self.take_current()?;
    if eos {
      self.sink.end();
    }
    Ok(())
  }

  fn flush(&mut self) -> Result<(), String> {
    self.queue.clear();
    self.current = None;
    // Drain what the decoder holds; the keyframe the seek lands on resets
    // its reference state.
    if let Err(e) = self.decoder.flush() {
      log::warn!("[forge::video] flush: {e}");
    }
    self.sink.flush();
    Ok(())
  }

  fn lead_ns(&self) -> i64 {
    sink_lead_ns(self.sink.as_ref())
  }

  fn set_playing(&mut self, playing: bool) {
    self.sink.set_playing(playing);
  }
}

#[cfg(not(target_os = "android"))]
impl<D: VideoDecoder> PresenterHost for DecoderPresenter<D> {
  fn presenter(&mut self) -> Box<dyn Presenter + '_> {
    Box::new(self)
  }
}
