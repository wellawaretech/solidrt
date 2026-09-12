// forge::video - the video playback capability core, engine-free (see
// okf/plans/android-video-punch-through.md and
// okf/backlog/video-playback.md). Demux produces coded VP9 frames from a
// container; two players consume them:
//
// - the TEXTURE player (player.rs): a decoder produces timestamped planar
//   YUV frames as plain CPU bytes and the consumer (alloy, via the flux
//   binding) uploads the planes as textures and converts YUV to RGB on the
//   GPU. Decoders are swappable producers of the same frames: MediaCodec
//   buffer mode on Android, libvpx everywhere else (Google's reference
//   decoder, vendored; VP9 is royalty-free, so a bundled software decoder
//   carries no codec licensing).
// - the PLANE player (plane.rs, Android): MediaCodec decodes straight into
//   a platform surface and the compositor presents it, off our frame loop
//   entirely. No frames ever cross into Rust.
//
// Both share the demuxer contract (`Demuxer`) and the transport
// (transport.rs: clock anchor, play/pause/seek, published state).
//
// No GL, SDL, or scripting-engine types anywhere in this module.

mod aac;
mod demux;
#[cfg(target_os = "android")]
mod mediacodec;
#[cfg(target_os = "android")]
mod plane;
mod player;
pub mod transport;
#[cfg(not(target_os = "android"))]
mod vpx;

pub use aac::{AacDecoder, PcmChunk};
pub use demux::{AudioInfo, AudioPacket, MediaInfo, Mp4Demuxer, VideoAu};
#[cfg(target_os = "android")]
pub use mediacodec::MediaCodecDecoder;
#[cfg(target_os = "android")]
pub use plane::PlanePlayer;
pub use player::VideoPlayer;
#[cfg(not(target_os = "android"))]
pub use vpx::Vp9Decoder;

/// A demultiplexed media source: coded VP9 frames and raw AAC packets, each
/// track in decode order, read from a byte source that may be unbounded (a
/// live stream) and may not seek. One implementation per container (MP4
/// today; WebM is the live round's); players never see a sample table.
pub trait Demuxer: Send {
  fn info(&self) -> &MediaInfo;
  /// Next coded video frame, None past the end (never, on a live stream).
  fn next_video(&mut self) -> Result<Option<VideoAu>, String>;
  /// Next raw AAC frame, None past the end or when there is no audio track.
  fn next_audio(&mut self) -> Result<Option<AudioPacket>, String>;
  /// Reposition so the next video frame is the last keyframe at or before
  /// `target_us` (or the next keyframe, for a source that cannot go back)
  /// and the next audio packet the first at or after `target_us`. A source
  /// that cannot seek at all errs; a player treats that as unsupported.
  fn seek(&mut self, target_us: i64) -> Result<(), String>;
}

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
