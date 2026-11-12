pub fn sdl_error() -> String {
  sdl3::get_error().to_string()
}

// --- Custom IOStream (feed SDL / SDL_mixer from an arbitrary Rust byte source;
// the sdl3 crate only wraps from_file/from_bytes/from_read, and the last reads
// the whole thing up front, so none of them can stream) --------------------

use std::io::{Read, Seek, SeekFrom};

use sdl3::iostream::IOStream;
use sdl3::sys::iostream::{SDL_IOStatus, SDL_IOStreamInterface, SDL_IOWhence, SDL_OpenIO};

/// The byte source behind a custom IOStream: seekable, and `Send` because SDL
/// reads it from its own decode thread.
trait ReadSeek: Read + Seek + Send {}
impl<T: Read + Seek + Send> ReadSeek for T {}

// `userdata` is a thin pointer to the boxed (fat) trait object.
type Reader = Box<dyn ReadSeek>;

unsafe extern "C" fn io_size(userdata: *mut std::ffi::c_void) -> i64 {
  let reader = &mut *(userdata as *mut Reader);
  // Report the total size without disturbing the read cursor.
  let Ok(cur) = reader.stream_position() else { return -1 };
  let Ok(end) = reader.seek(SeekFrom::End(0)) else { return -1 };
  if reader.seek(SeekFrom::Start(cur)).is_err() {
    return -1;
  }
  end as i64
}

unsafe extern "C" fn io_seek(userdata: *mut std::ffi::c_void, offset: i64, whence: SDL_IOWhence) -> i64 {
  let reader = &mut *(userdata as *mut Reader);
  let from = match whence {
    SDL_IOWhence::SET => SeekFrom::Start(offset as u64),
    SDL_IOWhence::CUR => SeekFrom::Current(offset),
    SDL_IOWhence::END => SeekFrom::End(offset),
    _ => return -1,
  };
  reader.seek(from).map(|pos| pos as i64).unwrap_or(-1)
}

unsafe extern "C" fn io_read(
  userdata: *mut std::ffi::c_void,
  ptr: *mut std::ffi::c_void,
  size: usize,
  status: *mut SDL_IOStatus,
) -> usize {
  let reader = &mut *(userdata as *mut Reader);
  let buf = std::slice::from_raw_parts_mut(ptr as *mut u8, size);
  match reader.read(buf) {
    Ok(0) => {
      *status = SDL_IOStatus::EOF;
      0
    }
    Ok(n) => n,
    Err(_) => {
      *status = SDL_IOStatus::ERROR;
      0
    }
  }
}

unsafe extern "C" fn io_close(userdata: *mut std::ffi::c_void) -> bool {
  // SDL is done with the stream: reclaim and drop the boxed reader.
  drop(Box::from_raw(userdata as *mut Reader));
  true
}

/// Wrap an arbitrary seekable byte source in an SDL IOStream so SDL_mixer (or
/// any SDL consumer) can pull from it on demand. SDL owns the reader once this
/// succeeds and drops it via the close callback when the IOStream is closed.
pub fn iostream_from_reader<R: Read + Seek + Send + 'static>(reader: R) -> Result<IOStream<'static>, String> {
  let boxed: Reader = Box::new(reader);
  // Box the (fat) trait-object box again so `userdata` is a thin pointer.
  let userdata = Box::into_raw(Box::new(boxed)) as *mut std::ffi::c_void;

  // `new()` stamps the interface version; we only supply the read paths.
  let mut iface = SDL_IOStreamInterface::new();
  iface.size = Some(io_size);
  iface.seek = Some(io_seek);
  iface.read = Some(io_read);
  iface.close = Some(io_close);

  let raw = unsafe { SDL_OpenIO(&iface, userdata) };
  if raw.is_null() {
    // SDL did not take ownership; reclaim the reader so it is not leaked.
    unsafe { drop(Box::from_raw(userdata as *mut Reader)) };
    return Err(sdl_error());
  }
  Ok(unsafe { IOStream::from_ll(raw) })
}
