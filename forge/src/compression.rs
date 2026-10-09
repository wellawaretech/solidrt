//! Engine-free deflate codec behind the Compression Streams API
//! (`CompressionStream` / `DecompressionStream` in flux): the three formats
//! the standard names, gzip (RFC 1952), deflate (the zlib format, RFC 1950)
//! and deflate-raw (RFC 1951), as one incremental `Codec`. Input goes in by
//! `Step::Push`, output comes back one piece per step, of at most
//! `OUTPUT_CHUNK_BYTES`, with `pending` saying whether another step would
//! produce more for what the codec already holds; so the host can hand each
//! piece on before asking for the next, and a tiny input that inflates to a
//! lot never runs ahead of its reader.
//!
//! The deflate engine is flate2's `Compress`/`Decompress` on the pure-Rust
//! backend already in the tree (miniz_oxide), which also does the zlib
//! framing. The gzip framing is done here: on that backend flate2's mem API
//! has no gzip mode (`new_gzip` is zlib-backend only), and the container is
//! a 10-byte header plus an 8-byte CRC-32/size trailer around a raw deflate
//! stream, checked with flate2's `Crc`.
//!
//! Errors are the standard's: corrupt input, input cut short at `Finish`,
//! and data after the end of the stream (a second gzip member included)
//! fail the step. A failed codec is not stepped again.

use std::sync::OnceLock;

use flate2::{Compress, Compression, Crc, Decompress, FlushCompress, FlushDecompress, Status};

use crate::workers::Workers;

/// The most bytes one step produces: the piece a reader gets per pull. Small
/// enough that a stream inflating a thousandfold stays a bounded step ahead
/// of its reader, large enough that a megabyte is a handful of steps.
pub const OUTPUT_CHUNK_BYTES: usize = 64 * 1024;

/// How many codec steps run at once, process-wide: a step is a short burst
/// of CPU, so a thread per open stream would only contend; the rest queue.
const CODEC_THREADS: usize = 2;

/// The gzip header this codec writes (RFC 1952, 2.3): the two id bytes,
/// method 8 (deflate), no flags, no modification time, no extra flags, OS
/// 255 (unknown). No name and no time, so the bytes are the same wherever
/// they are made.
const GZIP_HEADER: [u8; 10] = [GZIP_ID[0], GZIP_ID[1], GZIP_METHOD_DEFLATE, 0, 0, 0, 0, 0, 0, 255];
/// The fixed part of a gzip header, before the optional fields FLG names.
const GZIP_HEADER_FIXED: usize = 10;
/// The gzip trailer: CRC-32 and size mod 2^32 of the uncompressed data,
/// little-endian.
const GZIP_TRAILER: usize = 8;
const GZIP_ID: [u8; 2] = [0x1f, 0x8b];
const GZIP_METHOD_DEFLATE: u8 = 8;
// The FLG bits of a gzip header (RFC 1952, 2.3.1).
const FLG_FHCRC: u8 = 1 << 1;
const FLG_FEXTRA: u8 = 1 << 2;
const FLG_FNAME: u8 = 1 << 3;
const FLG_FCOMMENT: u8 = 1 << 4;
/// The reserved FLG bits: a header with any of them set is not gzip as the
/// standard defines it.
const FLG_RESERVED: u8 = 0xe0;

/// The formats the standard names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
  /// RFC 1952: a header and a CRC-32/size trailer around raw deflate.
  Gzip,
  /// The zlib format, RFC 1950: a two-byte header and an Adler-32 trailer.
  Deflate,
  /// RFC 1951, the bare deflate stream: what a ZIP entry holds.
  DeflateRaw,
}

impl Format {
  /// The names `parse` takes, for an error message.
  pub const NAMES: &'static str = "gzip, deflate, deflate-raw";

  /// The format the standard calls `name`, `None` for any other string.
  pub fn parse(name: &str) -> Option<Format> {
    match name {
      "gzip" => Some(Format::Gzip),
      "deflate" => Some(Format::Deflate),
      "deflate-raw" => Some(Format::DeflateRaw),
      _ => None,
    }
  }

  pub fn name(self) -> &'static str {
    match self {
      Format::Gzip => "gzip",
      Format::Deflate => "deflate",
      Format::DeflateRaw => "deflate-raw",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
  Compress,
  Decompress,
}

/// What a step does with the codec, besides producing the next piece.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
  /// More input.
  Push(Vec<u8>),
  /// Nothing new: the next piece for what the codec holds (after a step
  /// that left it `pending`).
  Next,
  /// The input has ended: the engine flushes, and a decompressor's input
  /// must have ended its stream. Further steps are `Next`.
  Finish,
}

/// Where the codec stands in the stream's framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
  /// Before the gzip header (gzip only): writing it, or reading it.
  Header,
  /// In the deflate stream.
  Body,
  /// After the deflate stream, the gzip trailer (gzip only): writing it, or
  /// reading it.
  Trailer,
  /// The stream has ended; any input after it is trailing data.
  Done,
}

enum Engine {
  Deflate(Compress),
  Inflate(Decompress),
}

/// The state around the engine: the framing, and the input the engine has
/// not consumed yet.
struct Frame {
  format: Format,
  /// Input not yet consumed: a step stops at `OUTPUT_CHUNK_BYTES` of output,
  /// and a gzip header or trailer waits here until it is whole.
  input: Vec<u8>,
  /// gzip: the CRC-32 and count of the uncompressed bytes so far, written
  /// as the trailer when compressing and checked against it when not.
  crc: Crc,
  stage: Stage,
  /// `Finish` was stepped: the engine flushes, the input must end the stream.
  finishing: bool,
  /// Another step would produce output without more input.
  pending: bool,
}

pub struct Codec {
  engine: Engine,
  frame: Frame,
}

impl Codec {
  pub fn new(format: Format, direction: Direction) -> Codec {
    let zlib_header = format == Format::Deflate;
    let engine = match direction {
      Direction::Compress => Engine::Deflate(Compress::new(Compression::default(), zlib_header)),
      Direction::Decompress => Engine::Inflate(Decompress::new(zlib_header)),
    };
    let stage = if format == Format::Gzip { Stage::Header } else { Stage::Body };
    let frame = Frame { format, input: Vec::new(), crc: Crc::new(), stage, finishing: false, pending: false };
    Codec { engine, frame }
  }

  /// Whether another step would produce output for what the codec already
  /// holds: the last piece filled up, or input waits unconsumed, or a
  /// finish is still flushing.
  pub fn pending(&self) -> bool {
    self.frame.pending
  }

  /// One step: take `step`, then produce the next piece of output, at most
  /// `OUTPUT_CHUNK_BYTES` (and possibly empty: a compressor buffers input
  /// until it has a block). Errs on corrupt input, on input cut short when
  /// finishing, and on data after the end of a stream.
  pub fn step(&mut self, step: Step) -> Result<Vec<u8>, String> {
    match step {
      Step::Push(bytes) => {
        if self.frame.input.is_empty() {
          self.frame.input = bytes;
        } else {
          self.frame.input.extend_from_slice(&bytes);
        }
      }
      Step::Next => {}
      Step::Finish => self.frame.finishing = true,
    }
    let mut out = vec![0u8; OUTPUT_CHUNK_BYTES];
    let len = match &mut self.engine {
      Engine::Deflate(engine) => self.frame.compress(engine, &mut out)?,
      Engine::Inflate(engine) => self.frame.decompress(engine, &mut out)?,
    };
    out.truncate(len);
    Ok(out)
  }

  /// `step` on the codec threads (`CODEC_THREADS` of them, shared by every
  /// stream in the process): the codec moves into the job and comes back
  /// with the piece, so no thread is held between steps. On an error the
  /// codec is gone with the job: it is not stepped again.
  pub async fn step_queued(mut self, step: Step) -> Result<(Codec, Vec<u8>), String> {
    static WORKERS: OnceLock<Workers> = OnceLock::new();
    let workers = WORKERS.get_or_init(|| Workers::new("deflate", CODEC_THREADS));
    workers
      .run(move || {
        let out = self.step(step)?;
        Ok((self, out))
      })
      .await?
  }
}

impl Frame {
  /// The next piece of compressed output into `out`; its length.
  fn compress(&mut self, engine: &mut Compress, out: &mut [u8]) -> Result<usize, String> {
    self.pending = false;
    if self.stage == Stage::Done {
      if self.input.is_empty() {
        return Ok(0);
      }
      return Err("input after the end of the stream".to_string());
    }
    let mut len = 0;
    if self.stage == Stage::Header {
      out[..GZIP_HEADER.len()].copy_from_slice(&GZIP_HEADER);
      len = GZIP_HEADER.len();
      self.stage = Stage::Body;
    }
    if self.stage == Stage::Body {
      let flush = if self.finishing { FlushCompress::Finish } else { FlushCompress::None };
      let (before_in, before_out) = (engine.total_in(), engine.total_out());
      let status = engine.compress(&self.input, &mut out[len..], flush).map_err(|e| format!("deflate failed ({e})"))?;
      let consumed = (engine.total_in() - before_in) as usize;
      len += (engine.total_out() - before_out) as usize;
      if self.format == Format::Gzip {
        self.crc.update(&self.input[..consumed]);
      }
      self.input.drain(..consumed);
      match status {
        Status::StreamEnd => self.stage = if self.format == Format::Gzip { Stage::Trailer } else { Stage::Done },
        Status::Ok => {}
        // Nothing to do: no input and no flush. While finishing the engine
        // always has something to do, so this would be a loop.
        Status::BufError if self.finishing => return Err("deflate made no progress".to_string()),
        Status::BufError => {}
      }
    }
    if self.stage == Stage::Trailer && out.len() - len >= GZIP_TRAILER {
      let trailer = &mut out[len..len + GZIP_TRAILER];
      trailer[..4].copy_from_slice(&self.crc.sum().to_le_bytes());
      trailer[4..].copy_from_slice(&self.crc.amount().to_le_bytes());
      len += GZIP_TRAILER;
      self.stage = Stage::Done;
    }
    self.pending = self.stage != Stage::Done && (self.finishing || !self.input.is_empty() || len == out.len());
    Ok(len)
  }

  /// The next piece of decompressed output into `out`; its length.
  fn decompress(&mut self, engine: &mut Decompress, out: &mut [u8]) -> Result<usize, String> {
    self.pending = false;
    let name = self.format.name();
    let cut_short = || format!("the {name} data is cut short");
    if self.stage == Stage::Header {
      match gzip_header_len(&self.input).map_err(|e| format!("the gzip data is corrupt ({e})"))? {
        Some(header) => {
          self.input.drain(..header);
          self.stage = Stage::Body;
        }
        None => return if self.finishing { Err(cut_short()) } else { Ok(0) },
      }
    }
    let mut len = 0;
    let mut body_pending = false;
    if self.stage == Stage::Body {
      let flush = if self.finishing { FlushDecompress::Finish } else { FlushDecompress::None };
      let (before_in, before_out) = (engine.total_in(), engine.total_out());
      let status =
        engine.decompress(&self.input, out, flush).map_err(|e| format!("the {name} data is corrupt ({e})"))?;
      let consumed = (engine.total_in() - before_in) as usize;
      len = (engine.total_out() - before_out) as usize;
      if self.format == Format::Gzip {
        self.crc.update(&out[..len]);
      }
      self.input.drain(..consumed);
      match status {
        Status::StreamEnd => self.stage = if self.format == Format::Gzip { Stage::Trailer } else { Stage::Done },
        // The piece filled up, or input waits that the engine stopped short
        // of: more may come without new input.
        Status::Ok => body_pending = len == out.len() || !self.input.is_empty(),
        // No progress: the engine needs more input.
        Status::BufError => {}
      }
    }
    if self.stage == Stage::Trailer && self.input.len() >= GZIP_TRAILER {
      let crc = u32::from_le_bytes([self.input[0], self.input[1], self.input[2], self.input[3]]);
      let size = u32::from_le_bytes([self.input[4], self.input[5], self.input[6], self.input[7]]);
      if crc != self.crc.sum() {
        return Err("the gzip data is corrupt (CRC mismatch)".to_string());
      }
      if size != self.crc.amount() {
        return Err("the gzip data is corrupt (size mismatch)".to_string());
      }
      self.input.drain(..GZIP_TRAILER);
      self.stage = Stage::Done;
    }
    if self.stage == Stage::Done {
      if !self.input.is_empty() {
        return Err(format!("trailing data after the end of the {name} stream"));
      }
      return Ok(len);
    }
    self.pending = body_pending;
    if self.finishing && !self.pending {
      return Err(cut_short());
    }
    Ok(len)
  }
}

/// The length of the gzip header at the start of `bytes` (RFC 1952, 2.3):
/// `None` while more bytes are needed to tell, an error for what is not a
/// gzip header. The optional header CRC is skipped, not checked, as zlib
/// does.
fn gzip_header_len(bytes: &[u8]) -> Result<Option<usize>, String> {
  for (at, expected) in GZIP_ID.iter().enumerate() {
    if bytes.get(at).is_some_and(|b| b != expected) {
      return Err("not a gzip stream".to_string());
    }
  }
  if bytes.get(2).is_some_and(|&method| method != GZIP_METHOD_DEFLATE) {
    return Err("unsupported compression method".to_string());
  }
  if bytes.len() < GZIP_HEADER_FIXED {
    return Ok(None);
  }
  let flags = bytes[3];
  if flags & FLG_RESERVED != 0 {
    return Err("reserved header flags set".to_string());
  }
  let mut at = GZIP_HEADER_FIXED;
  if flags & FLG_FEXTRA != 0 {
    let Some(xlen) = bytes.get(at..at + 2) else { return Ok(None) };
    at += 2 + u16::from_le_bytes([xlen[0], xlen[1]]) as usize;
    if bytes.len() < at {
      return Ok(None);
    }
  }
  for flag in [FLG_FNAME, FLG_FCOMMENT] {
    if flags & flag != 0 {
      match bytes.get(at..).unwrap_or_default().iter().position(|&b| b == 0) {
        Some(end) => at += end + 1,
        None => return Ok(None),
      }
    }
  }
  if flags & FLG_FHCRC != 0 {
    at += 2;
    if bytes.len() < at {
      return Ok(None);
    }
  }
  Ok(Some(at))
}
