// The Android hardware decoder for textures: AMediaCodec in buffer mode,
// `video/x-vnd.on2.vp9`, as the presenter the shared worker drives
// (worker.rs), plus the codec setup and input step the plane's surface
// presenter shares. Probed on the Philips TPM171E 2026-08-12 with AVC (see
// okf/backlog/video-playback.md): the buffer tap emits an honest layout
// (color-format 21 = NV12, stride/slice-height padded); surface-attached
// taps are per-device untrustworthy and are not used. The TV's VP9 decoder
// (OMX.MTK.VIDEO.DECODER.VP9, up to 4096x2304) is the same OMX family, and
// every Android device has at least the platform's software VP9 decoder
// behind this mime. VP9 has no out-of-band parameter sets, so there is no
// csd: the codec takes the container's samples as they are, superframes
// included.
//
// The codec's padded output (stride, slice-height, crop) is repacked into
// the tightly packed frame contract during the mandatory copy out of the
// codec buffer. Planar output (color-format 19) is interleaved to NV12 in
// the same pass, so every device feeds the same layout downstream.

use std::time::Duration;

use ndk::media::media_codec::{
  DequeuedInputBufferResult, DequeuedOutputBufferInfoResult, MediaCodec, MediaCodecDirection,
};
use ndk::media::media_format::MediaFormat;
use ndk::native_window::NativeWindow;

use super::reader::{Next, Reader};
use super::texture::sink_lead_ns;
use super::transport::FrameSink;
use super::worker::{Feed, Picture, Presenter, PresenterHost};
use super::{PixelLayout, StreamError, YuvFrame};

/// The MediaCodec mime for VP9 (MediaFormat.MIMETYPE_VIDEO_VP9).
pub(crate) const MIME_VP9: &str = "video/x-vnd.on2.vp9";
pub(crate) const FLAG_END_OF_STREAM: u32 = ndk_sys::AMEDIACODEC_BUFFER_FLAG_END_OF_STREAM as u32;
const FLAG_CODEC_CONFIG: u32 = ndk_sys::AMEDIACODEC_BUFFER_FLAG_CODEC_CONFIG as u32;

// A hardware decoder is a limited resource: the target TV allows two VP9
// instances, so switching clips - where the incoming player is built before
// the outgoing one is closed - can find them both taken. Retry across that
// handover rather than failing a clip for a codec that is about to be free.
// Total wait stays well inside the time a player takes to start.
const DECODER_ATTEMPTS: u32 = 10;
const DECODER_RETRY_MS: u64 = 50;

/// Run a codec constructor with the clip-handover retry above (used by both
/// decoder modes). The last error is reported when every attempt fails.
pub(crate) fn create_with_retry<T>(mut make: impl FnMut() -> Result<T, String>) -> Result<T, String> {
  let mut last = String::new();
  for attempt in 0..DECODER_ATTEMPTS {
    match make() {
      Ok(value) => return Ok(value),
      Err(e) => last = e,
    }
    if attempt + 1 < DECODER_ATTEMPTS {
      std::thread::sleep(Duration::from_millis(DECODER_RETRY_MS));
    }
  }
  Err(format!("{last} (after {DECODER_ATTEMPTS} attempts)"))
}
/// Create and start a VP9 decoder for a `width` x `height` stream: in
/// surface mode when `window` is given (the plane), buffer mode otherwise.
pub(crate) fn open_codec(window: Option<&NativeWindow>, width: u32, height: u32) -> Result<MediaCodec, String> {
  let codec = MediaCodec::from_decoder_type(MIME_VP9).ok_or_else(|| format!("no {MIME_VP9} decoder on this device"))?;
  let mut format = MediaFormat::new();
  format.set_str("mime", MIME_VP9);
  format.set_i32("width", width as i32);
  format.set_i32("height", height as i32);
  codec
    .configure(&format, window, MediaCodecDirection::Decoder)
    .map_err(|e| format!("configure {MIME_VP9} decoder: {e:?}"))?;
  codec.start().map_err(|e| format!("start {MIME_VP9} decoder: {e:?}"))?;
  Ok(codec)
}

/// The codec input step both presenters share (`Presenter::feed`): queue
/// one coded frame from the reader, or the end of the stream, into a free
/// input buffer. The queue is looked at before a buffer is dequeued: an
/// input buffer dequeued with nothing to put in it is lost until the next
/// flush (ndk's InputBuffer has no Drop). A frame that does not fit the
/// buffer is skipped (fail-soft) rather than the buffer lost.
pub(crate) fn feed_codec(codec: &MediaCodec, reader: &Reader) -> Result<Feed, StreamError> {
  let ready = match reader.video_ready() {
    Next::Packet(()) => true,
    Next::End => false,
    Next::Waiting => return Ok(Feed::Waiting),
  };
  let mut buf = match codec.dequeue_input_buffer(Duration::ZERO) {
    Ok(DequeuedInputBufferResult::Buffer(buf)) => buf,
    Ok(DequeuedInputBufferResult::TryAgainLater) => return Ok(Feed::Refused { packet_waiting: ready }),
    Err(e) => return Err(StreamError::decode(format!("dequeue input: {e:?}"))),
  };
  let next = if ready {
    loop {
      match reader.next_video() {
        Next::Packet(au) => {
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
        Next::End => break None,
        // A seek emptied the queue since the look: the buffer is lost until
        // the flush that seek brings next.
        Next::Waiting => return Ok(Feed::Waiting),
      }
    }
  } else {
    None
  };
  match next {
    Some(au) => {
      let target = buf.buffer_mut();
      for (dst, src) in target.iter_mut().zip(&au.data) {
        dst.write(*src);
      }
      codec
        .queue_input_buffer(buf, 0, au.data.len(), au.pts_us.max(0) as u64, 0)
        .map_err(|e| StreamError::decode(format!("queue input: {e:?}")))?;
      Ok(Feed::Took)
    }
    None => {
      codec
        .queue_input_buffer(buf, 0, 0, 0, FLAG_END_OF_STREAM)
        .map_err(|e| StreamError::decode(format!("queue input: {e:?}")))?;
      Ok(Feed::End)
    }
  }
}

// MediaCodecInfo.CodecCapabilities color formats (Java-level constants, no
// ndk-sys symbols): 21 = YUV420SemiPlanar (NV12), 19 = YUV420Planar (I420).
const COLOR_NV12: i32 = 21;
const COLOR_I420: i32 = 19;

/// Geometry of the codec's output buffers, read from the output format at
/// the format-changed event (which precedes the first buffer). Crop keys are
/// read as plain i32s: `format.rect()` is API-28 gated and the TV is API 26.
struct OutputFacts {
  color_format: i32,
  stride: usize,
  slice_height: usize,
  crop_left: usize,
  crop_top: usize,
  width: u32,
  height: u32,
}

/// The buffer-mode codec over a frame sink: `feed` is the shared input
/// step, `next` dequeues one output, repacks it into a tightly packed frame
/// and returns the codec buffer at once, holding the frame until the worker
/// releases it to the sink or discards it. Owns everything it needs, so it
/// is its own host.
pub(crate) struct BufferPresenter {
  codec: MediaCodec,
  configured: (u32, u32),
  facts: Option<OutputFacts>,
  sink: Box<dyn FrameSink>,
  // The picture made current by `next`, with whether it is the stream's
  // last (the end goes to the sink when it is released or discarded).
  current: Option<(Option<YuvFrame>, bool)>,
}

impl BufferPresenter {
  pub(crate) fn new(codec: MediaCodec, width: u32, height: u32, sink: Box<dyn FrameSink>) -> Self {
    BufferPresenter { codec, configured: (width, height), facts: None, sink, current: None }
  }

  fn take_current(&mut self) -> Result<(Option<YuvFrame>, bool), String> {
    self.current.take().ok_or_else(|| "no picture is current".to_string())
  }
}

impl Presenter for BufferPresenter {
  fn feed(&mut self, reader: &Reader) -> Result<Feed, StreamError> {
    feed_codec(&self.codec, reader)
  }

  fn next(&mut self, wait: Duration) -> Result<Option<Picture>, StreamError> {
    if self.current.is_some() {
      return Err(StreamError::decode("a picture is already current"));
    }
    loop {
      match self.codec.dequeue_output_buffer(wait).map_err(|e| StreamError::decode(format!("dequeue output: {e:?}")))? {
        DequeuedOutputBufferInfoResult::Buffer(buf) => {
          let info = *buf.info();
          let eos = info.flags() & FLAG_END_OF_STREAM != 0;
          let pts_us = info.presentation_time_us();
          let frame = if info.size() > 0 && info.flags() & FLAG_CODEC_CONFIG == 0 {
            if self.facts.is_none() {
              // Some codecs skip the format-changed event; read on demand.
              self.facts = Some(read_facts(&self.codec, self.configured).map_err(StreamError::decode)?);
            }
            let facts = self.facts.as_ref().expect("facts read above");
            match repack(facts, buf.buffer(), info.offset() as usize, pts_us) {
              Ok(frame) => Some(frame),
              // A buffer that cannot be repacked is skipped (fail-soft).
              Err(e) => {
                log::warn!("[forge::video] skipping frame at {pts_us}us: {e}");
                None
              }
            }
          } else {
            None
          };
          self
            .codec
            .release_output_buffer(buf, false)
            .map_err(|e| StreamError::decode(format!("release output: {e:?}")))?;
          match frame {
            Some(frame) => {
              self.current = Some((Some(frame), eos));
              return Ok(Some(Picture { pts_us, eos, empty: false }));
            }
            None if eos => {
              self.current = Some((None, true));
              return Ok(Some(Picture { pts_us, eos: true, empty: true }));
            }
            None => continue,
          }
        }
        DequeuedOutputBufferInfoResult::OutputFormatChanged => {
          self.facts = Some(read_facts(&self.codec, self.configured).map_err(StreamError::decode)?);
        }
        DequeuedOutputBufferInfoResult::OutputBuffersChanged => {}
        DequeuedOutputBufferInfoResult::TryAgainLater => return Ok(None),
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
    self.current = None;
    self.codec.flush().map_err(|e| format!("flush: {e:?}"))?;
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

impl PresenterHost for BufferPresenter {
  fn presenter(&mut self) -> Box<dyn Presenter + '_> {
    Box::new(self)
  }
}

fn read_facts(codec: &MediaCodec, configured: (u32, u32)) -> Result<OutputFacts, String> {
  let format = codec.output_format();
  let color_format = format.i32("color-format").ok_or("output format missing color-format")?;
  let coded_w = format.i32("width").unwrap_or(configured.0 as i32);
  let coded_h = format.i32("height").unwrap_or(configured.1 as i32);
  let stride = format.i32("stride").filter(|&v| v > 0).unwrap_or(coded_w) as usize;
  let slice_height = format.i32("slice-height").filter(|&v| v > 0).unwrap_or(coded_h) as usize;
  let crop_left = format.i32("crop-left").unwrap_or(0).max(0) as usize;
  let crop_top = format.i32("crop-top").unwrap_or(0).max(0) as usize;
  // Display size: the crop when present, the configured stream size
  // otherwise (the probed TV emits no crop keys and pads height to 1088;
  // the container's size is the display truth there).
  let (width, height) = match (format.i32("crop-right"), format.i32("crop-bottom")) {
    (Some(right), Some(bottom)) => {
      ((right - crop_left as i32 + 1).max(0) as u32, (bottom - crop_top as i32 + 1).max(0) as u32)
    }
    _ => configured,
  };
  Ok(OutputFacts { color_format, stride, slice_height, crop_left, crop_top, width, height })
}

/// Repack one padded codec buffer into a tightly packed NV12 frame (see
/// `PixelLayout`), honoring stride, slice-height, and crop offsets. Chroma
/// crops land on even pixels for 4:2:0 content.
fn repack(f: &OutputFacts, src: &[u8], offset: usize, pts_us: i64) -> Result<YuvFrame, String> {
  let (w, h) = (f.width as usize, f.height as usize);
  let (cw, ch) = (f.width.div_ceil(2) as usize, f.height.div_ceil(2) as usize);
  let src = src.get(offset..).ok_or("output buffer offset out of bounds")?;
  let y_base = f.crop_top * f.stride + f.crop_left;
  let chroma_base = f.stride * f.slice_height;
  let mut data = Vec::with_capacity(PixelLayout::Nv12.frame_size(f.width, f.height));
  match f.color_format {
    COLOR_NV12 => {
      let uv_base = chroma_base + (f.crop_top / 2) * f.stride + f.crop_left;
      let need = (uv_base + ch.saturating_sub(1) * f.stride + cw * 2).max(y_base + h.saturating_sub(1) * f.stride + w);
      if src.len() < need {
        return Err(format!("output buffer too small: {} < {need}", src.len()));
      }
      for row in 0..h {
        data.extend_from_slice(&src[y_base + row * f.stride..][..w]);
      }
      for row in 0..ch {
        data.extend_from_slice(&src[uv_base + row * f.stride..][..cw * 2]);
      }
    }
    COLOR_I420 => {
      let cstride = f.stride / 2;
      let cslice = f.slice_height / 2;
      let u_base = chroma_base + (f.crop_top / 2) * cstride + f.crop_left / 2;
      let v_base = chroma_base + cstride * cslice + (f.crop_top / 2) * cstride + f.crop_left / 2;
      let need = (v_base + ch.saturating_sub(1) * cstride + cw).max(y_base + h.saturating_sub(1) * f.stride + w);
      if src.len() < need {
        return Err(format!("output buffer too small: {} < {need}", src.len()));
      }
      for row in 0..h {
        data.extend_from_slice(&src[y_base + row * f.stride..][..w]);
      }
      for row in 0..ch {
        let u = &src[u_base + row * cstride..][..cw];
        let v = &src[v_base + row * cstride..][..cw];
        for i in 0..cw {
          data.push(u[i]);
          data.push(v[i]);
        }
      }
    }
    other => return Err(format!("unsupported decoder color-format {other} (expected 21 NV12 or 19 planar)")),
  }
  Ok(YuvFrame { pts_us, width: f.width, height: f.height, layout: PixelLayout::Nv12, data })
}
