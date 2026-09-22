// forge::video - the video playback capability core, engine-free (see
// okf/plans/android-video-punch-through.md and
// okf/backlog/video-playback.md). The container is WebM (webm.rs): coded
// VP9 frames and Opus packets come out of it as they are stored, and two
// players consume them:
//
// - the TEXTURE player (texture.rs): a decoder produces timestamped planar
//   YUV frames as plain CPU bytes, pushed with the time each is due to the
//   consumer's frame sink (alloy's YUV texture latch, via the flux binding),
//   which uploads the planes as textures and converts YUV to RGB on the
//   GPU when the frame is due. Decoders are swappable producers of the same
//   frames: MediaCodec buffer mode on Android (mediacodec.rs), libvpx
//   everywhere else (Google's reference decoder, vendored; VP9 is
//   royalty-free, so a bundled software decoder carries no codec
//   licensing).
// - the PLANE player (plane.rs, Android): MediaCodec decodes straight into
//   a platform surface and the compositor presents it. No frames ever
//   cross into Rust.
//
// Neither runs anything on the caller's thread: both are the one worker
// (worker.rs) over their presenter, off the frame loop entirely.
//
// Audio is Opus through the vendored libopus (opus/) on every platform.
// Both players share the demuxer contract (`Demuxer`), the reader thread
// (reader.rs), the transport (transport.rs: clock anchor, play/pause/seek,
// published state, the audio and frame sink contracts), the audio track
// feeder (audio.rs) and the playback worker (worker.rs: one thread over a
// `Presenter`, the piece that differs per player).
//
// No GL, SDL, or scripting-engine types anywhere in this module.

pub(crate) mod audio;
#[cfg(target_os = "android")]
mod mediacodec;
mod opus;
#[cfg(target_os = "android")]
mod plane;
pub mod reader;
mod texture;
pub mod transport;
#[cfg(not(target_os = "android"))]
mod vpx;
mod webm;
pub mod worker;

use std::fmt;

pub use opus::{OpusDecoder, PcmChunk};
#[cfg(target_os = "android")]
pub use plane::open as open_plane;
pub use texture::open as open_texture;
pub use transport::{AudioSink, Clock, FrameSink};
#[cfg(not(target_os = "android"))]
pub use vpx::Vp9Decoder;
pub use webm::WebmDemuxer;
pub use worker::Player;

/// What went wrong with a stream, for the app to key on: the same kinds
/// reach JS as `VideoError.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
  /// The source could not be opened or stopped delivering: an unreachable
  /// or failing server, an unreadable file, a truncated body.
  Network,
  /// The bytes are not a stream the decoders can follow: a corrupt
  /// container, a codec that failed on it.
  Decode,
  /// A stream, codec or feature this player does not play.
  Unsupported,
  /// No video plane: not on this platform, or one is open already.
  NoPlane,
  /// A read interrupted by the reading thread's own command (a seek, a
  /// close). Internal: never published to a player's caller.
  Interrupted,
}

impl ErrorKind {
  /// The kind's name on the JS surface.
  pub fn name(self) -> &'static str {
    match self {
      ErrorKind::Network => "network",
      ErrorKind::Decode => "decode",
      ErrorKind::Unsupported => "unsupported",
      ErrorKind::NoPlane => "no-plane",
      ErrorKind::Interrupted => "interrupted",
    }
  }
}

/// A stream failure: its kind and a message for the log and the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamError {
  pub kind: ErrorKind,
  pub message: String,
}

impl StreamError {
  pub fn new(kind: ErrorKind, message: impl Into<String>) -> StreamError {
    StreamError { kind, message: message.into() }
  }

  pub fn network(message: impl Into<String>) -> StreamError {
    StreamError::new(ErrorKind::Network, message)
  }

  pub fn decode(message: impl Into<String>) -> StreamError {
    StreamError::new(ErrorKind::Decode, message)
  }

  pub fn unsupported(message: impl Into<String>) -> StreamError {
    StreamError::new(ErrorKind::Unsupported, message)
  }

  /// The same error with `prefix: ` in front of the message.
  pub fn context(self, prefix: &str) -> StreamError {
    StreamError { kind: self.kind, message: format!("{prefix}: {}", self.message) }
  }
}

impl fmt::Display for StreamError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.message)
  }
}

// A bare message is a decode error: the container said something the
// walk could not follow. The other kinds are named where they arise.
impl From<&str> for StreamError {
  fn from(message: &str) -> StreamError {
    StreamError::decode(message)
  }
}

impl From<String> for StreamError {
  fn from(message: String) -> StreamError {
    StreamError::decode(message)
  }
}

/// Stream facts read from the container header and the source.
#[derive(Clone, Debug)]
pub struct MediaInfo {
  pub width: u32,
  pub height: u32,
  /// None when the source does not say: a live stream, or a container
  /// whose Segment has no known size (a muxer writing to a pipe).
  pub duration_us: Option<i64>,
  /// Where playback starts: the first keyframe's pts, 0 for a whole file,
  /// later for a stream cut from a longer one. Audio before it is
  /// discarded by the players.
  pub start_us: i64,
  /// Whether `seek` can do anything; false means it errs and playback
  /// carries on where it was.
  pub seekable: bool,
  /// The conversion matrix: the container's when it says, else BT.709 for
  /// HD (720 lines and up) and BT.601 below.
  pub bt709: bool,
  /// Full-range (0..255) samples rather than studio range.
  pub full_range: bool,
  pub audio: Option<AudioInfo>,
}

/// The Opus track's facts: what a sink is opened at, and the two trims the
/// stream asks for (RFC 7845): the priming samples at the very start, and
/// how far before a seek target decoding has to begin to converge.
#[derive(Clone, Debug)]
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

/// One packet of either track, in the order the container stores them.
pub enum Packet {
  Video(VideoAu),
  Audio(AudioPacket),
}

/// A demultiplexed media source: coded VP9 frames and Opus packets, each
/// track in decode order, read from a byte source that may be unbounded (a
/// live stream) and may not seek. Players never see a container.
pub trait Demuxer: Send {
  fn info(&self) -> &MediaInfo;
  /// Next coded video frame, None past the end (never, on a live stream).
  fn next_video(&mut self) -> Result<Option<VideoAu>, StreamError>;
  /// Next Opus packet, None past the end or when there is no audio track.
  fn next_audio(&mut self) -> Result<Option<AudioPacket>, StreamError>;
  /// Next packet of either played track in container order, None past the
  /// end. What a reader filling both queues at once takes, so neither
  /// track is read ahead of the other.
  fn next_packet(&mut self) -> Result<Option<Packet>, StreamError>;
  /// Reposition so the next video frame is the last keyframe at or before
  /// `target_us` (or the next keyframe, for a source that cannot go back)
  /// and the next audio packet the first at or after the resume position
  /// minus the track's seek preroll. Returns the resume position: the
  /// target, or that next keyframe when it comes later. The player skips
  /// video and discards decoded audio before it. A source that cannot seek
  /// errs without touching its state; a player treats that as unsupported.
  fn seek(&mut self, target_us: i64) -> Result<i64, StreamError>;
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

/// A synchronous video decoder (the texture presenter's, see texture.rs):
/// feed one coded frame at a time, collect zero or more decoded frames (a
/// decoder may hold pictures back), then `flush` at end of stream for the
/// remainder. Errors are per-frame and recoverable - the caller may skip
/// the frame and continue (fail-soft playback policy). An asynchronous
/// codec (MediaCodec) is a presenter of its own instead.
pub trait VideoDecoder {
  fn decode(&mut self, au: &VideoAu) -> Result<Vec<YuvFrame>, String>;
  fn flush(&mut self) -> Result<Vec<YuvFrame>, String>;
}
