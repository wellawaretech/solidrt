// forge::video - the video playback capability core, engine-free (see
// okf/backlog/video-playback.md). One pipeline on every platform: demux
// produces coded VP9 frames, a decoder produces timestamped planar YUV
// frames as plain CPU bytes, and the consumer (alloy, via the flux binding)
// uploads the planes as textures and converts YUV to RGB on the GPU.
// Decoders are swappable producers of the same frames: MediaCodec on
// Android (hardware VP9 on the target TV), libvpx everywhere else (Google's
// reference decoder, vendored; VP9 is royalty-free, so a bundled software
// decoder carries no codec licensing, which is what made the H.264 era
// platform-only). The other hardware rungs (VA-API, VideoToolbox, Media
// Foundation) stay optimizations for later.
//
// No GL, SDL, or scripting-engine types anywhere in this module.

mod aac;
mod demux;
#[cfg(target_os = "android")]
mod mediacodec;
mod player;
#[cfg(not(target_os = "android"))]
mod vpx;

pub use aac::{AacDecoder, PcmChunk};
pub use demux::{AudioInfo, AudioPacket, MediaInfo, Mp4Demuxer, VideoAu};
#[cfg(target_os = "android")]
pub use mediacodec::MediaCodecDecoder;
pub use player::VideoPlayer;
#[cfg(not(target_os = "android"))]
pub use vpx::Vp9Decoder;

/// The layout the platform's decoder emits. Fixed per platform so consumers
/// can size textures before the first decoded frame exists: NV12 from
/// MediaCodec (its buffer-mode output, copied out row by row), I420 from
/// libvpx (its native planar output). alloy converts either on the GPU, so
/// neither path pays for a repack into the other's layout.
pub fn decoded_layout() -> PixelLayout {
  if cfg!(target_os = "android") {
    PixelLayout::Nv12
  } else {
    PixelLayout::I420
  }
}

/// Plane arrangement of a tightly packed YUV 4:2:0 frame: plane rows are
/// exactly the plane width, planes follow each other with no padding, chroma
/// dimensions round up. Matches alloy's YUV texture packing; producers with
/// padded output (decoder stride/slice-height) repack during the copy out of
/// the decoder's buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelLayout {
  /// Y plane, then one interleaved UV plane at half resolution (MediaCodec
  /// buffer-mode output).
  Nv12,
  /// Y plane, then U, then V, each at half resolution (libvpx output).
  I420,
}

impl PixelLayout {
  /// Byte length of one tightly packed frame at display size.
  pub fn frame_size(self, width: u32, height: u32) -> usize {
    // Both layouts carry the same bytes, arranged differently.
    let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
    width as usize * height as usize + cw * ch * 2
  }
}

/// One decoded frame: presentation timestamp in microseconds, display
/// dimensions, and the tightly packed plane bytes.
pub struct YuvFrame {
  pub pts_us: i64,
  pub width: u32,
  pub height: u32,
  pub layout: PixelLayout,
  pub data: Vec<u8>,
}

/// A video decoder: feed one coded frame at a time, collect zero or more
/// decoded frames (a decoder may hold pictures back), then `flush` at end
/// of stream for the remainder. Errors are per-frame and recoverable - the
/// caller may skip the frame and continue (fail-soft playback policy).
pub trait VideoDecoder {
  fn decode(&mut self, au: &VideoAu) -> Result<Vec<YuvFrame>, String>;
  fn flush(&mut self) -> Result<Vec<YuvFrame>, String>;
}
