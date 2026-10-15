// WAV: RIFF framing around raw PCM. A `fmt ` chunk says the layout, a
// `data` chunk holds interleaved samples, and anything else (LIST, cue,
// smpl, bext, JUNK, a pad byte after an odd-sized chunk) is skipped by
// size. The layouts every tool exports play: PCM 8, 16, 24 and 32 bit and
// 32-bit float, in a plain `fmt ` or the WAVE_FORMAT_EXTENSIBLE one most
// tools write for 24-bit and float. Compressed WAV (ADPCM, the G.711 laws,
// MPEG) and RF64 are rejected by name: a clip in one of those is a
// re-export away, and guessing at it would play noise.

use std::io::{Read, Seek, SeekFrom};

use super::{AudioDecoder, AudioFormat};
use crate::seek::SeekableReader;

// Format tags of the `fmt ` chunk.
const FORMAT_PCM: u16 = 0x0001;
const FORMAT_IEEE_FLOAT: u16 = 0x0003;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
// The twelve bytes every KSDATAFORMAT_SUBTYPE_* GUID shares after its
// leading format tag; a different tail is a subformat we do not know.
const SUBTYPE_GUID_TAIL: [u8; 14] =
  [0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71];
// The `fmt ` chunk's fixed part, and the size an extensible one needs to
// carry its subformat.
const FMT_BASE_LEN: u32 = 16;
const FMT_EXTENSIBLE_LEN: u32 = 40;
// A `data` size a writer streaming to a pipe leaves unset: the data runs
// to the end of the file.
const SIZE_UNKNOWN: u32 = 0xFFFF_FFFF;
// Frames read per pass of `read`, bounding the byte buffer.
const READ_CHUNK_FRAMES: usize = 4096;

/// The sample layouts this reader converts to f32.
#[derive(Clone, Copy)]
enum Sample {
  U8,
  S16,
  S24,
  S32,
  F32,
}

impl Sample {
  fn bytes(self) -> usize {
    match self {
      Sample::U8 => 1,
      Sample::S16 => 2,
      Sample::S24 => 3,
      Sample::S32 => 4,
      Sample::F32 => 4,
    }
  }

  fn convert(self, bytes: &[u8]) -> f32 {
    match self {
      Sample::U8 => (bytes[0] as f32 - 128.0) / 128.0,
      Sample::S16 => i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0,
      // Sign-extend by placing the 24 bits in the top of an i32.
      Sample::S24 => (i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]) >> 8) as f32 / 8_388_608.0,
      Sample::S32 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / 2_147_483_648.0,
      Sample::F32 => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
  }
}

pub struct WavDecoder {
  reader: SeekableReader,
  sample_rate: u32,
  channels: u16,
  sample: Sample,
  // The `data` chunk's payload: where it starts and how many whole frames
  // it holds (clamped to the file).
  data_start: u64,
  frames: u64,
  cursor: u64,
  buf: Vec<u8>,
}

impl WavDecoder {
  /// Open a WAV from the start of `reader`; errs on anything but PCM or
  /// float WAV, naming what it found.
  pub fn open(mut reader: SeekableReader) -> Result<WavDecoder, String> {
    let len = reader.seek(SeekFrom::End(0)).map_err(|e| format!("wav: {e}"))?;
    reader.seek(SeekFrom::Start(0)).map_err(|e| format!("wav: {e}"))?;
    let mut riff = [0u8; 12];
    reader.read_exact(&mut riff).map_err(|_| "wav: file shorter than a RIFF header".to_string())?;
    if &riff[..4] == b"RF64" {
      return Err("wav: RF64 files (over 4 GB) are not supported".into());
    }
    if &riff[..4] != b"RIFF" || &riff[8..] != b"WAVE" {
      return Err("wav: no RIFF/WAVE header".into());
    }
    let mut fmt: Option<Format> = None;
    let mut pos = 12u64;
    loop {
      let mut header = [0u8; 8];
      if reader.read_exact(&mut header).is_err() {
        return Err(if fmt.is_some() { "wav: no data chunk" } else { "wav: no fmt chunk" }.into());
      }
      let id = &header[..4];
      let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
      pos += 8;
      match id {
        b"fmt " => {
          let mut body = vec![0u8; size as usize];
          reader.read_exact(&mut body).map_err(|_| "wav: truncated fmt chunk".to_string())?;
          fmt = Some(parse_format(&body)?);
        }
        b"data" => {
          let format = fmt.ok_or("wav: data chunk before the fmt chunk")?;
          let available = len.saturating_sub(pos);
          let bytes = if size == SIZE_UNKNOWN {
            available
          } else if size as u64 > available {
            log::warn!(
              "[forge::audio] wav data chunk says {size} bytes, file holds {available}: playing what is there"
            );
            available
          } else {
            size as u64
          };
          let frame_bytes = format.sample.bytes() as u64 * format.channels as u64;
          return Ok(WavDecoder {
            reader,
            sample_rate: format.sample_rate,
            channels: format.channels,
            sample: format.sample,
            data_start: pos,
            frames: bytes / frame_bytes,
            cursor: 0,
            buf: Vec::new(),
          });
        }
        _ => {
          // Chunks are word-aligned: an odd size carries a pad byte.
          let skip = size as u64 + (size & 1) as u64;
          reader.seek(SeekFrom::Current(skip as i64)).map_err(|e| format!("wav: {e}"))?;
        }
      }
      pos += size as u64 + (size & 1) as u64;
    }
  }
}

struct Format {
  sample_rate: u32,
  channels: u16,
  sample: Sample,
}

/// The `fmt ` chunk body: tag, channels, rate, byte rate, block align, bits,
/// and for WAVE_FORMAT_EXTENSIBLE the subformat GUID carrying the real tag.
fn parse_format(body: &[u8]) -> Result<Format, String> {
  if (body.len() as u32) < FMT_BASE_LEN {
    return Err("wav: fmt chunk too short".into());
  }
  let u16_at = |at: usize| u16::from_le_bytes([body[at], body[at + 1]]);
  let u32_at = |at: usize| u32::from_le_bytes([body[at], body[at + 1], body[at + 2], body[at + 3]]);
  let mut tag = u16_at(0);
  let channels = u16_at(2);
  let sample_rate = u32_at(4);
  let block_align = u16_at(12);
  let bits = u16_at(14);
  if tag == FORMAT_EXTENSIBLE {
    if (body.len() as u32) < FMT_EXTENSIBLE_LEN {
      return Err("wav: extensible fmt chunk too short".into());
    }
    if body[26..40] != SUBTYPE_GUID_TAIL {
      return Err("wav: unknown extensible subformat; only PCM and float WAV play".into());
    }
    tag = u16_at(24);
  }
  let sample = match (tag, bits) {
    (FORMAT_PCM, 8) => Sample::U8,
    (FORMAT_PCM, 16) => Sample::S16,
    (FORMAT_PCM, 24) => Sample::S24,
    (FORMAT_PCM, 32) => Sample::S32,
    (FORMAT_PCM, _) => return Err(format!("wav: {bits}-bit PCM is not supported (8, 16, 24 or 32)")),
    (FORMAT_IEEE_FLOAT, 32) => Sample::F32,
    (FORMAT_IEEE_FLOAT, _) => return Err(format!("wav: {bits}-bit float is not supported (32 only)")),
    (other, _) => return Err(format!("wav: {} is not supported; only PCM and float WAV play", format_name(other))),
  };
  if channels == 0 || channels > 2 {
    return Err(format!("wav: {channels} channels is not supported; only mono and stereo play"));
  }
  if sample_rate == 0 {
    return Err("wav: sample rate is 0".into());
  }
  if block_align as usize != sample.bytes() * channels as usize {
    return Err(format!("wav: block align {block_align} does not match {bits}-bit x {channels} channels"));
  }
  Ok(Format { sample_rate, channels, sample })
}

/// The name of a format tag this reader rejects, for the error.
fn format_name(tag: u16) -> String {
  match tag {
    0x0002 => "MS ADPCM".into(),
    0x0006 => "A-law".into(),
    0x0007 => "mu-law".into(),
    0x0011 => "IMA ADPCM".into(),
    0x0050 | 0x0055 => "MPEG audio".into(),
    other => format!("format tag 0x{other:04x}"),
  }
}

impl AudioDecoder for WavDecoder {
  fn format(&self) -> AudioFormat {
    AudioFormat { sample_rate: self.sample_rate, channels: self.channels, frames: Some(self.frames) }
  }

  fn read(&mut self, out: &mut [f32]) -> Result<usize, String> {
    let channels = self.channels as usize;
    let frame_bytes = self.sample.bytes() * channels;
    let wanted = (out.len() / channels).min((self.frames - self.cursor) as usize).min(READ_CHUNK_FRAMES);
    if wanted == 0 {
      return Ok(0);
    }
    self.buf.resize(wanted * frame_bytes, 0);
    // A short read is the file ending early; the frames in hand play.
    let mut filled = 0;
    while filled < self.buf.len() {
      match self.reader.read(&mut self.buf[filled..]) {
        Ok(0) => break,
        Ok(n) => filled += n,
        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
        Err(e) => return Err(format!("wav: {e}")),
      }
    }
    let frames = filled / frame_bytes;
    if frames < wanted {
      self.frames = self.cursor + frames as u64;
    }
    let width = self.sample.bytes();
    for (i, sample) in out[..frames * channels].iter_mut().enumerate() {
      *sample = self.sample.convert(&self.buf[i * width..(i + 1) * width]);
    }
    self.cursor += frames as u64;
    Ok(frames)
  }

  fn seek(&mut self, frame: u64) -> Result<(), String> {
    let frame = frame.min(self.frames);
    let frame_bytes = self.sample.bytes() as u64 * self.channels as u64;
    self.reader.seek(SeekFrom::Start(self.data_start + frame * frame_bytes)).map_err(|e| format!("wav: {e}"))?;
    self.cursor = frame;
    Ok(())
  }
}
