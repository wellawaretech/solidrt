// forge::video - the video playback capability core, engine-free (see
// okf/plans/android-video-punch-through.md and
// okf/backlog/video-playback.md). The container is WebM (webm.rs): coded
// VP9 frames and Opus packets come out of it as they are stored, and two
// players consume them:
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
// Audio is Opus through the vendored libopus (opus/) on every platform.
// Both players share the demuxer contract (`Demuxer`), the transport
// (transport.rs: clock anchor, play/pause/seek, published state, the audio
// sink contract) and the audio track feeder (audio.rs).
//
// No GL, SDL, or scripting-engine types anywhere in this module.

pub(crate) mod audio;
#[cfg(target_os = "android")]
mod mediacodec;
mod opus;
#[cfg(target_os = "android")]
mod plane;
mod player;
pub mod transport;
#[cfg(not(target_os = "android"))]
mod vpx;
mod webm;

#[cfg(target_os = "android")]
pub use mediacodec::MediaCodecDecoder;
pub use opus::{OpusDecoder, PcmChunk};
#[cfg(target_os = "android")]
pub use plane::PlanePlayer;
pub use player::VideoPlayer;
pub use transport::AudioSink;
#[cfg(not(target_os = "android"))]
pub use vpx::Vp9Decoder;
pub use webm::WebmDemuxer;

/// Stream facts read from the container header.
#[derive(Clone)]
pub struct MediaInfo {
  pub width: u32,
  pub height: u32,
  /// None when the source does not say (a live stream).
  pub duration_us: Option<i64>,
  pub audio: Option<AudioInfo>,
}

/// The Opus track's facts: what a sink is opened at, and the two trims the
/// stream asks for (RFC 7845): the priming samples at the very start, and
/// how far before a seek target decoding has to begin to converge.
#[derive(Clone)]
pub struct AudioInfo {
  pub sample_rate: u32,
  pub channels: u16,
  /// Samples (per channel) to discard from the start of the stream.
  pub pre_skip: u32,
  /// After a seek, packets from this long before the target are decoded
  /// and their output discarded.
  pub seek_preroll_us: i64,
}

/// One coded VP9 frame (a superframe when the container packs a hidden
/// ALT-REF with its shown frame), exactly as stored; decoders split
/// superframes themselves. `sync` marks a keyframe.
pub struct VideoAu {
  pub pts_us: i64,
  pub sync: bool,
  pub data: Vec<u8>,
}

/// One Opus packet.
pub struct AudioPacket {
  pub pts_us: i64,
  pub data: Vec<u8>,
}

/// A demultiplexed media source: coded VP9 frames and Opus packets, each
/// track in decode order, read from a byte source that may be unbounded (a
/// live stream) and may not seek. Players never see a container.
pub trait Demuxer: Send {
  fn info(&self) -> &MediaInfo;
  /// Next coded video frame, None past the end (never, on a live stream).
  fn next_video(&mut self) -> Result<Option<VideoAu>, String>;
  /// Next Opus packet, None past the end or when there is no audio track.
  fn next_audio(&mut self) -> Result<Option<AudioPacket>, String>;
  /// Reposition so the next video frame is the last keyframe at or before
  /// `target_us` (or the next keyframe, for a source that cannot go back)
  /// and the next audio packet the first at or after `target_us` minus the
  /// track's seek preroll (the packets before the target are decoded and
  /// discarded by the player). A source that cannot seek at all errs; a
  /// player treats that as unsupported.
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
