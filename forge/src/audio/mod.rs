//! Clip decoding: WAV, Opus in Ogg (.opus) and Opus in WebM, to interleaved
//! f32. Engine-free and device-free: a headless `flux` decodes a file here
//! without alloy, and alloy's mixer plays what comes out. One trait covers
//! the three containers, so a decoder streams (the mixer's decode thread
//! pulls it) or decodes whole (`decode`). The Opus decoder itself (the
//! vendored libopus) lives here too; video's audio track uses it.
//!
//! Formats are Opus and WAV by decision (okf/plans/audio-unification.md):
//! one codec across video and audio, and WAV needs no codec. Anything else
//! is refused by name, so a Vorbis file says to re-encode as Opus rather
//! than failing on a malformed header.

mod opus;
mod opusfile;
// The Opus discard rule has two users, video's audio track and the WebM clip
// decoder below, both video's; an audio-only build has nothing to trim.
#[cfg(feature = "video")]
pub(crate) mod trim;
mod wav;
#[cfg(feature = "video")]
pub(crate) mod webm;

use std::io::{Cursor, Read, Seek, SeekFrom};

pub use opus::{OpusDecoder, PcmChunk};

use crate::seek::SeekableReader;

// The magic bytes the opener sniffs: RIFF/RF64 ... WAVE, an Ogg page, an
// EBML header.
const MAGIC_LEN: usize = 12;
const RIFF: &[u8] = b"RIFF";
const RF64: &[u8] = b"RF64";
const WAVE: &[u8] = b"WAVE";
const OGG_PAGE: &[u8] = b"OggS";
const EBML: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];
// Where the first packet of a one-segment first Ogg page starts (the
// 27-byte page header plus a one-entry segment table), and how a Vorbis
// identification header begins there.
const OGG_FIRST_PACKET: usize = 28;
const VORBIS_ID: &[u8] = b"\x01vorbis";
// Frames read per pass when decoding a whole clip.
const DECODE_CHUNK_FRAMES: usize = 4096;

/// A whole decoded clip: interleaved f32 at the clip's own rate.
pub struct Pcm {
  pub sample_rate: u32,
  pub channels: u16,
  pub samples: Vec<f32>,
}

impl Pcm {
  pub fn frames(&self) -> usize {
    self.samples.len() / self.channels as usize
  }
}

/// What a decoder produces: its rate and channel count, and its length in
/// frames when the container says (None for a source without a known end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioFormat {
  pub sample_rate: u32,
  pub channels: u16,
  pub frames: Option<u64>,
}

/// A clip being decoded on request, in order, with seeking. Interleaved f32
/// at `format()`'s rate and channel count; a whole number of frames per
/// read.
pub trait AudioDecoder: Send {
  fn format(&self) -> AudioFormat;
  /// Decode into `out` (whole frames; a trailing partial frame of room is
  /// left unused). Returns the frames written: fewer than fit when a pass
  /// of the source yields fewer, 0 at the end.
  fn read(&mut self, out: &mut [f32]) -> Result<usize, String>;
  /// Reposition so the next read starts at `frame` (clamped to the end).
  fn seek(&mut self, frame: u64) -> Result<(), String>;
}

/// Open a clip from the start of `reader`, by its magic bytes: WAV, Opus in
/// Ogg, or Opus in WebM (with the `video` feature). Errs naming what the
/// bytes are when they are something else.
pub fn open(mut reader: SeekableReader) -> Result<Box<dyn AudioDecoder>, String> {
  let mut magic = [0u8; MAGIC_LEN];
  let mut have = 0;
  while have < MAGIC_LEN {
    match reader.read(&mut magic[have..]) {
      Ok(0) => break,
      Ok(n) => have += n,
      Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
      Err(e) => return Err(format!("audio: {e}")),
    }
  }
  let magic = &magic[..have];
  // The Vorbis check reads a little further; every path rewinds first.
  let vorbis = magic.starts_with(OGG_PAGE) && {
    let mut id = [0u8; VORBIS_ID.len()];
    reader.seek(SeekFrom::Start(OGG_FIRST_PACKET as u64)).is_ok()
      && reader.read_exact(&mut id).is_ok()
      && id == VORBIS_ID
  };
  reader.seek(SeekFrom::Start(0)).map_err(|e| format!("audio: {e}"))?;
  if magic.len() >= MAGIC_LEN && (magic.starts_with(RIFF) || magic.starts_with(RF64)) && &magic[8..12] == WAVE {
    return Ok(Box::new(wav::WavDecoder::open(reader)?));
  }
  if magic.starts_with(OGG_PAGE) {
    if vorbis {
      return Err("audio: Ogg Vorbis is not supported; encode the clip as Opus (.opus)".into());
    }
    return Ok(Box::new(opusfile::OpusFileDecoder::open(reader)?));
  }
  if magic.starts_with(&EBML) {
    #[cfg(feature = "video")]
    return Ok(Box::new(webm::WebmAudioDecoder::open(reader)?));
    #[cfg(not(feature = "video"))]
    return Err("audio: WebM audio needs the video feature".into());
  }
  Err("audio: unrecognized format (WAV, Opus in Ogg or WebM play)".into())
}

/// Unsigned 8-bit samples (128 = silence) as f32 in [-1, 1).
pub fn samples_from_u8(bytes: &[u8]) -> Vec<f32> {
  bytes.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect()
}

/// Signed 16-bit samples as f32 in [-1, 1).
pub fn samples_from_i16(samples: &[i16]) -> Vec<f32> {
  samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// Decode a whole clip held in memory.
pub fn decode(bytes: &[u8]) -> Result<Pcm, String> {
  let mut decoder = open(Box::new(Cursor::new(bytes.to_vec())))?;
  let format = decoder.format();
  let channels = format.channels as usize;
  let mut samples = Vec::with_capacity(format.frames.unwrap_or(0) as usize * channels);
  let mut chunk = vec![0.0f32; DECODE_CHUNK_FRAMES * channels];
  loop {
    let frames = decoder.read(&mut chunk)?;
    if frames == 0 {
      break;
    }
    samples.extend_from_slice(&chunk[..frames * channels]);
  }
  Ok(Pcm { sample_rate: format.sample_rate, channels: format.channels, samples })
}
