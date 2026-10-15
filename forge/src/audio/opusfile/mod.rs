// Opus in Ogg (.opus, RFC 7845) through the vendored libopusfile, Xiph's
// reference reader, bound by hand in ffi.rs. The library does the whole
// job: Ogg pages and packets, the OpusHead and OpusTags, pre-skip, the
// header's output gain, chained streams, and seeking by bisection. Output
// is interleaved f32 at the 48 kHz every Opus file is defined at. The
// byte source is any `SeekableReader`, reached through read/seek/tell
// callbacks that own it for the file's lifetime.

mod ffi;

use std::ffi::{c_int, c_uchar, c_void};
use std::io::{Read, Seek, SeekFrom};

use super::{AudioDecoder, AudioFormat};
use crate::seek::SeekableReader;

// Every Opus file decodes at this rate (RFC 7845).
const OPUS_SAMPLE_RATE: u32 = 48_000;
// The fseek whence values the seek callback receives (the C standard's).
const SEEK_SET: c_int = 0;
const SEEK_CUR: c_int = 1;
const SEEK_END: c_int = 2;

// The callbacks hand libopusfile a `*mut SeekableReader` as the stream.
unsafe extern "C" fn read_cb(stream: *mut c_void, ptr: *mut c_uchar, nbytes: c_int) -> c_int {
  let reader = &mut *(stream as *mut SeekableReader);
  let buf = std::slice::from_raw_parts_mut(ptr, nbytes.max(0) as usize);
  loop {
    match reader.read(buf) {
      Ok(n) => return n as c_int,
      Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
      Err(_) => return -1,
    }
  }
}

unsafe extern "C" fn seek_cb(stream: *mut c_void, offset: i64, whence: c_int) -> c_int {
  let reader = &mut *(stream as *mut SeekableReader);
  let from = match whence {
    SEEK_SET => SeekFrom::Start(offset.max(0) as u64),
    SEEK_CUR => SeekFrom::Current(offset),
    SEEK_END => SeekFrom::End(offset),
    _ => return -1,
  };
  if reader.seek(from).is_ok() {
    0
  } else {
    -1
  }
}

unsafe extern "C" fn tell_cb(stream: *mut c_void) -> i64 {
  let reader = &mut *(stream as *mut SeekableReader);
  reader.stream_position().map(|pos| pos as i64).unwrap_or(-1)
}

unsafe extern "C" fn close_cb(stream: *mut c_void) -> c_int {
  drop(Box::from_raw(stream as *mut SeekableReader));
  0
}

const CALLBACKS: ffi::OpusFileCallbacks =
  ffi::OpusFileCallbacks { read: Some(read_cb), seek: Some(seek_cb), tell: Some(tell_cb), close: Some(close_cb) };

pub struct OpusFileDecoder {
  of: *mut ffi::OggOpusFile,
  channels: u16,
  frames: Option<u64>,
  cursor: u64,
  // Whether a hole in the data has been reported for this file.
  hole_logged: bool,
}

// The file is used from one thread at a time (the owner's); the reader
// inside it is Send, and libopusfile keeps no global state.
unsafe impl Send for OpusFileDecoder {}

impl OpusFileDecoder {
  /// Open an Opus file from the start of `reader`. A chained file plays as
  /// one, as long as every link has the same channel count.
  pub fn open(reader: SeekableReader) -> Result<OpusFileDecoder, String> {
    let stream = Box::into_raw(Box::new(reader)) as *mut c_void;
    let mut error: c_int = 0;
    let of = unsafe { ffi::op_open_callbacks(stream, &CALLBACKS, std::ptr::null(), 0, &mut error) };
    if of.is_null() {
      // A failed open does not call the close callback: the reader is ours
      // to free.
      unsafe { drop(Box::from_raw(stream as *mut SeekableReader)) };
      return Err(format!("opus: open failed: {}", describe(error)));
    }
    let channels = unsafe { ffi::op_channel_count(of, -1) };
    let seekable = unsafe { ffi::op_seekable(of) } != 0;
    if seekable {
      let links = unsafe { ffi::op_link_count(of) };
      for li in 0..links {
        if unsafe { ffi::op_channel_count(of, li) } != channels {
          unsafe { ffi::op_free(of) };
          return Err("opus: a chained file changes its channel count between links".into());
        }
      }
    }
    if !(1..=2).contains(&channels) {
      unsafe { ffi::op_free(of) };
      return Err(format!("opus: {channels} channels is not supported; only mono and stereo play"));
    }
    let total = if seekable { unsafe { ffi::op_pcm_total(of, -1) } } else { -1 };
    Ok(OpusFileDecoder {
      of,
      channels: channels as u16,
      frames: (total >= 0).then_some(total as u64),
      cursor: 0,
      hole_logged: false,
    })
  }
}

impl AudioDecoder for OpusFileDecoder {
  fn format(&self) -> AudioFormat {
    AudioFormat { sample_rate: OPUS_SAMPLE_RATE, channels: self.channels, frames: self.frames }
  }

  fn read(&mut self, out: &mut [f32]) -> Result<usize, String> {
    let channels = self.channels as usize;
    let room = out.len() / channels * channels;
    let mut filled = 0usize;
    while filled < room {
      let rest = &mut out[filled..room];
      let n = unsafe { ffi::op_read_float(self.of, rest.as_mut_ptr(), rest.len() as c_int, std::ptr::null_mut()) };
      if n > 0 {
        filled += n as usize * channels;
      } else if n == 0 {
        break;
      } else if n == ffi::OP_HOLE {
        if !self.hole_logged {
          self.hole_logged = true;
          log::warn!("[forge::audio] opus: a hole in the data; some samples were skipped");
        }
      } else {
        return Err(format!("opus: read failed: {}", describe(n)));
      }
    }
    let frames = filled / channels;
    self.cursor += frames as u64;
    Ok(frames)
  }

  fn seek(&mut self, frame: u64) -> Result<(), String> {
    let frame = match self.frames {
      Some(total) => frame.min(total),
      None => frame,
    };
    let result = unsafe { ffi::op_pcm_seek(self.of, frame as i64) };
    if result != 0 {
      return Err(format!("opus: seek failed: {}", describe(result)));
    }
    self.cursor = frame;
    Ok(())
  }
}

impl Drop for OpusFileDecoder {
  fn drop(&mut self) {
    // Closes the stream through the close callback, freeing the reader.
    unsafe { ffi::op_free(self.of) };
  }
}

/// A description of a libopusfile return code (the library has no
/// strerror).
fn describe(code: c_int) -> String {
  match code {
    ffi::OP_EREAD => "the source could not be read".into(),
    ffi::OP_EFAULT => "out of memory".into(),
    ffi::OP_EIMPL => "the stream uses a feature libopusfile does not implement".into(),
    ffi::OP_EINVAL => "invalid argument (a seek past the end, or the file is only partially open)".into(),
    ffi::OP_ENOTFORMAT => "not an Opus file (no Opus stream in the Ogg container)".into(),
    ffi::OP_EBADHEADER => "malformed Opus header".into(),
    ffi::OP_EVERSION => "unknown Opus header version".into(),
    ffi::OP_ENOTAUDIO => "not an audio stream".into(),
    ffi::OP_EBADPACKET => "a packet failed to decode".into(),
    ffi::OP_EBADLINK => "the file changed under the reader".into(),
    ffi::OP_ENOSEEK => "the source cannot seek".into(),
    ffi::OP_EBADTIMESTAMP => "a link starts at an invalid timestamp".into(),
    other => format!("error {other}"),
  }
}
