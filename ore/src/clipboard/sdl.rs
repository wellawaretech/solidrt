//! The SDL clipboard calls, over `sdl3::sys`: the sdl3 crate's safe
//! clipboard needs a `VideoSubsystem` handle ore does not own, and it has
//! no data calls. SDL's clipboard is main thread only, and so is every
//! function here: reach them through `Session`, which calls them on the
//! bridge.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;

use sdl3::sys::clipboard::{
  SDL_ClearClipboardData, SDL_GetClipboardData, SDL_GetClipboardText, SDL_SetClipboardData, SDL_SetClipboardText,
};
use sdl3::sys::error::SDL_ClearError;
use sdl3::sys::init::{SDL_WasInit, SDL_INIT_VIDEO};
use sdl3::sys::stdinc::SDL_free;

/// The check every call makes first: without the video subsystem SDL's
/// reads answer as if the clipboard were empty while its writes fail, so
/// all four answer this one error instead.
fn video_ready() -> Result<(), String> {
  // SAFETY: SDL_WasInit takes plain flags and reads SDL's init state.
  if unsafe { SDL_WasInit(SDL_INIT_VIDEO) }.0 == 0 {
    return Err("the clipboard needs the video subsystem, which is not initialized".to_string());
  }
  Ok(())
}

/// SDL's error after a failed call, or `fallback` when the call set none
/// (Wayland fails a write without one when there is no seat or data
/// device). The error is cleared before each call, so it is never stale.
fn error_or(fallback: &str) -> String {
  let error = crate::sdl_error();
  if error.is_empty() {
    fallback.to_string()
  } else {
    error
  }
}

/// Replace the clipboard's content with `text`; an empty text clears it.
/// Main thread only.
pub(super) fn set_text(text: &str) -> Result<(), String> {
  video_ready()?;
  let text = CString::new(text).map_err(|_| "the text contains a NUL character".to_string())?;
  // SAFETY: `text` is a valid C string that outlives the call; SDL copies
  // it. SDL_ClearError takes no arguments.
  let set = unsafe {
    SDL_ClearError();
    SDL_SetClipboardText(text.as_ptr())
  };
  if set {
    Ok(())
  } else {
    Err(error_or("the platform did not take the text"))
  }
}

/// The clipboard's text, "" when it holds none. Main thread only.
pub(super) fn text() -> Result<String, String> {
  video_ready()?;
  // SAFETY: takes no arguments; the result is null or a C string SDL
  // allocated for us. SDL_ClearError takes no arguments.
  let raw = unsafe {
    SDL_ClearError();
    SDL_GetClipboardText()
  };
  if raw.is_null() {
    return Err(error_or("the platform did not answer with the text"));
  }
  // SAFETY: `raw` is a non-null, NUL-terminated string SDL allocated; it is
  // copied out, then freed once with SDL_free as SDL's docs ask.
  let text = unsafe {
    let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
    SDL_free(raw.cast::<c_void>());
    text
  };
  Ok(text)
}

/// The clipboard's data of type `mime`, None when it holds none: one
/// call, since asking first whether the data is there fetches it too on
/// X11, and the owner can change between two calls. Main thread only.
pub(super) fn data(mime: &CStr) -> Result<Option<Vec<u8>>, String> {
  video_ready()?;
  let mut size: usize = 0;
  // SAFETY: `mime` is a valid C string and `size` a valid out-parameter,
  // both outliving the call. The error is cleared first, so a null result
  // with no error set reads as "no data" rather than a stale message.
  let raw = unsafe {
    SDL_ClearError();
    SDL_GetClipboardData(mime.as_ptr(), &mut size)
  };
  if raw.is_null() {
    let error = crate::sdl_error();
    return if error.is_empty() { Ok(None) } else { Err(error) };
  }
  // SAFETY: SDL returns `size` readable bytes at `raw`; they are copied
  // out, then freed once with SDL_free.
  let bytes = unsafe {
    let bytes = std::slice::from_raw_parts(raw.cast::<u8>(), size).to_vec();
    SDL_free(raw);
    bytes
  };
  // A zero-length answer (macOS and Windows can give one) is no data.
  Ok((!bytes.is_empty()).then_some(bytes))
}

/// Offer `bytes` as the clipboard's one type `mime`, in place of what it
/// held. SDL keeps the bytes until the clipboard is next replaced or
/// cleared, or SDL quits. Main thread only.
pub(super) fn set_data(mime: &CStr, bytes: Arc<[u8]>) -> Result<(), String> {
  // SDL_SetClipboardData (SDL 3.4.18, src/video/SDL_clipboard.c) returns
  // before it takes the userdata only without a video device or for
  // invalid parameters; ours are always valid, so the one case to rule out
  // is video not being up, which `video_ready` does before anything is
  // boxed.
  video_ready()?;
  let mime_types: [*const c_char; 1] = [mime.as_ptr()];
  let userdata = Box::into_raw(Box::new(bytes)).cast::<c_void>();
  // SAFETY: `provide` and `release` match the callback signatures, and
  // `userdata` is the `Box<Arc<[u8]>>` both expect. `mime_types` holds one
  // valid C string and outlives the call; SDL copies the list. Past its
  // video and parameter checks SDL_SetClipboardData has stored callback,
  // cleanup and userdata before anything can fail (and every later failure
  // is either cleaned up by SDL itself, through SDL_ClearClipboardData and
  // so SDL_CancelClipboardData's cleanup call, or leaves the userdata
  // stored), so from here SDL owns the box and frees it through `release`
  // when the clipboard is next replaced, cleared, or at quit. The box is
  // never freed here, whatever the call returns. SDL_ClearError takes no
  // arguments.
  let offered = unsafe {
    SDL_ClearError();
    SDL_SetClipboardData(Some(provide), Some(release), userdata, mime_types.as_ptr(), 1)
  };
  if offered {
    return Ok(());
  }
  let error = error_or("the platform did not take the image");
  // SAFETY: takes no arguments. SDL_ClearClipboardData sets empty data,
  // which runs SDL_CancelClipboardData and so `release` on the stored box
  // (or nothing, when SDL already released it), freeing the bytes at once.
  unsafe { SDL_ClearClipboardData() };
  Err(error)
}

/// SDL's data callback: the offered bytes, whatever the type asked for
/// (one type is offered). Must not panic: it runs under SDL's C frames.
unsafe extern "C" fn provide(userdata: *mut c_void, _mime: *const c_char, size: *mut usize) -> *const c_void {
  if userdata.is_null() {
    return std::ptr::null();
  }
  // SAFETY: `userdata` is the `Box<Arc<[u8]>>` `set_data` handed SDL,
  // alive until `release` runs, which SDL never does while it calls this.
  let bytes = unsafe { &*userdata.cast::<Arc<[u8]>>() };
  if !size.is_null() {
    // SAFETY: SDL passes a valid out-parameter for the length.
    unsafe { *size = bytes.len() };
  }
  bytes.as_ptr().cast::<c_void>()
}

/// SDL's cleanup callback: drop the offered bytes. Must not panic.
unsafe extern "C" fn release(userdata: *mut c_void) {
  if userdata.is_null() {
    return;
  }
  // SAFETY: `userdata` is the box `set_data` leaked; SDL runs the cleanup
  // once, when it drops the data (SDL_CancelClipboardData).
  drop(unsafe { Box::from_raw(userdata.cast::<Arc<[u8]>>()) });
}
