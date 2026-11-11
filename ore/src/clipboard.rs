//! The clipboard: text, and a PNG image, copied to and pasted from other
//! apps. SDL does it (main thread only, so every call goes through the
//! bridge): X11 and Wayland selections on Linux, NSPasteboard on macOS
//! (`image/png` as its PNG type), the Win32 clipboard on Windows
//! (`image/png` as the registered "PNG" format), the activity's
//! ClipboardManager on Android, which carries text only.
//!
//! On Linux the app serves what it copied: another app reads the data
//! from this process when it pastes, so a copy, text included, is gone
//! when the app exits unless a clipboard manager took it over. On macOS
//! SDL offers an image through a data provider the app answers when
//! another app pastes, and writes text at once, a special case its source
//! gives "so text is available after the app quits": an image copy there
//! is served by the app too, and likely gone when it exits (not checked
//! on a Mac). A written PNG stays in memory until the clipboard is next
//! replaced or cleared, by this app or another.
//!
//! An image read answers the PNG when the clipboard holds one. On
//! Windows a source like the PrintScreen key puts only a bitmap there
//! (`CF_DIB`/`CF_DIBV5`, which SDL hands over as a BMP file); the read
//! then answers that BMP with its format. ore does not convert it: the
//! layer above does, so no image codec lives here.
//!
//! `Session` is the session an embedder keeps per engine, the shape of
//! `link::Session`: it holds one value, the content of the clipboard as
//! the app last wrote it, which an agent reads back; a session set to
//! record only never reaches the OS and that value is its clipboard, as
//! a test host's engine has it.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

mod sdl;

/// The eight bytes every PNG file starts with (the PNG specification,
/// section 5.2): what `write_image` checks before it offers the bytes.
pub(crate) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The encoding of an image on the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
  Png,
  /// A BMP file: what a Windows bitmap (`CF_DIB`) reads as.
  Bmp,
}

impl ImageFormat {
  pub fn mime(self) -> &'static str {
    match self {
      ImageFormat::Png => "image/png",
      ImageFormat::Bmp => "image/bmp",
    }
  }

  pub fn from_mime(mime: &str) -> Option<ImageFormat> {
    match mime {
      "image/png" => Some(ImageFormat::Png),
      "image/bmp" => Some(ImageFormat::Bmp),
      _ => None,
    }
  }

  fn c_mime(self) -> &'static CStr {
    match self {
      ImageFormat::Png => c"image/png",
      ImageFormat::Bmp => c"image/bmp",
    }
  }
}

/// An encoded image and its format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
  pub format: ImageFormat,
  pub bytes: Arc<[u8]>,
}

/// What the clipboard holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
  Text(String),
  Image(Image),
}

/// Why a clipboard call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardError {
  /// This platform carries no image on its clipboard.
  Unavailable,
  /// The bytes to write are not a PNG file.
  NotPng,
  /// The platform could not read or write the clipboard.
  Failed(String),
}

impl std::fmt::Display for ClipboardError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ClipboardError::Unavailable => f.write_str("no image clipboard on this platform"),
      ClipboardError::NotPng => f.write_str("the bytes are not a PNG image"),
      ClipboardError::Failed(message) => f.write_str(message),
    }
  }
}

/// What a clipboard call is answered through, once, from the thread it
/// finished on.
pub type Responder<T> = Box<dyn FnOnce(Result<T, ClipboardError>) + Send>;

/// Whether this platform can write and read an image on its clipboard:
/// the desktops. SDL's Android clipboard is text only.
pub fn image_available() -> bool {
  cfg!(any(target_os = "linux", target_os = "macos", target_os = "windows"))
}

/// The clipboard session of one engine (see the module doc).
pub struct Session {
  record_only: AtomicBool,
  held: Mutex<Option<Content>>,
}

impl Session {
  pub fn new() -> Arc<Session> {
    Arc::new(Session { record_only: AtomicBool::new(false), held: Mutex::new(None) })
  }

  /// Never reach the OS clipboard in this session: the session's own
  /// value is the clipboard, written and read at once (a test host's
  /// engine, a render).
  pub fn set_record_only(&self) {
    self.record_only.store(true, Ordering::Relaxed);
  }

  pub fn is_record_only(&self) -> bool {
    self.record_only.load(Ordering::Relaxed)
  }

  /// In a record-only session, what the clipboard holds. In any other,
  /// what the app last wrote successfully, for an agent to read back: not
  /// the OS clipboard's content, which another app may have replaced
  /// since.
  pub fn held(&self) -> Option<Content> {
    self.held.lock().unwrap_or_else(PoisonError::into_inner).clone()
  }

  /// Put `content` on the session's clipboard, in place of what it held:
  /// what a test does before the app pastes. An empty text clears it.
  pub fn put(&self, content: Option<Content>) {
    let content = content.filter(|content| !matches!(content, Content::Text(text) if text.is_empty()));
    *self.held.lock().unwrap_or_else(PoisonError::into_inner) = content;
  }

  /// Replace the clipboard's content with `text`; an empty text clears
  /// it, as SDL does. A text with a NUL character is refused first, in a
  /// record-only session too, since the OS clipboard cannot take it.
  pub fn write_text(self: &Arc<Self>, text: String, respond: Responder<()>) {
    if text.contains('\0') {
      return respond(Err(ClipboardError::Failed("the text contains a NUL character".to_string())));
    }
    if self.is_record_only() {
      self.put(Some(Content::Text(text)));
      return respond(Ok(()));
    }
    let session = Arc::clone(self);
    crate::main_thread::run(move |on_main| {
      let result = match on_main {
        Err(error) => Err(ClipboardError::Failed(error)),
        Ok(()) => {
          let written = sdl::set_text(&text).map_err(ClipboardError::Failed);
          session.settle_write(&written, Content::Text(text));
          written
        }
      };
      respond(result);
    });
  }

  /// The clipboard's text: "" when it is empty or holds no text.
  pub fn read_text(&self, respond: Responder<String>) {
    if self.is_record_only() {
      let text = match self.held() {
        Some(Content::Text(text)) => text,
        _ => String::new(),
      };
      return respond(Ok(text));
    }
    crate::main_thread::run(move |on_main| {
      respond(on_main.map_err(ClipboardError::Failed).and_then(|()| sdl::text().map_err(ClipboardError::Failed)));
    });
  }

  /// Replace the clipboard's content with the PNG file `png`. The bytes
  /// stay in memory until the clipboard is next replaced or cleared.
  pub fn write_image(self: &Arc<Self>, png: Arc<[u8]>, respond: Responder<()>) {
    if !png.starts_with(&PNG_SIGNATURE) {
      return respond(Err(ClipboardError::NotPng));
    }
    let image = Image { format: ImageFormat::Png, bytes: png };
    if self.is_record_only() {
      self.put(Some(Content::Image(image)));
      return respond(Ok(()));
    }
    if !image_available() {
      return respond(Err(ClipboardError::Unavailable));
    }
    let session = Arc::clone(self);
    crate::main_thread::run(move |on_main| {
      let result = match on_main {
        Err(error) => Err(ClipboardError::Failed(error)),
        Ok(()) => {
          let written = sdl::set_data(image.format.c_mime(), image.bytes.clone()).map_err(ClipboardError::Failed);
          session.settle_write(&written, Content::Image(image));
          written
        }
      };
      respond(result);
    });
  }

  /// Mirror an OS write into `held`: the content once written, and
  /// nothing after any failure of the sdl call, the video check included,
  /// where SDL never ran. Conservative: after a failed write the session
  /// claims nothing. Where the app serves the data (X11, Wayland, an image
  /// on macOS) the earlier content is gone, since SDL_SetClipboardData
  /// cancels the old data first; on Windows and for text on macOS a failed
  /// write can leave the earlier content on the OS clipboard.
  fn settle_write(&self, result: &Result<(), ClipboardError>, content: Content) {
    self.put(result.is_ok().then_some(content));
  }

  /// The clipboard's image, None when it holds none: the PNG, or on
  /// Windows the bitmap when there is no PNG (see the module doc).
  pub fn read_image(&self, respond: Responder<Option<Image>>) {
    if self.is_record_only() {
      let image = match self.held() {
        Some(Content::Image(image)) => Some(image),
        _ => None,
      };
      return respond(Ok(image));
    }
    if !image_available() {
      return respond(Err(ClipboardError::Unavailable));
    }
    crate::main_thread::run(move |on_main| {
      respond(on_main.map_err(ClipboardError::Failed).and_then(|()| image_now()));
    });
  }
}

/// The image formats a read asks for, in order: the PNG, then on Windows
/// the bitmap, as SDL hands `CF_DIB`/`CF_DIBV5` over (a BMP file).
#[cfg(target_os = "windows")]
const READ_FORMATS: &[ImageFormat] = &[ImageFormat::Png, ImageFormat::Bmp];
#[cfg(not(target_os = "windows"))]
const READ_FORMATS: &[ImageFormat] = &[ImageFormat::Png];

/// The clipboard's image now: the first format of `READ_FORMATS` it
/// holds, each asked for in one call. Main thread only.
fn image_now() -> Result<Option<Image>, ClipboardError> {
  for &format in READ_FORMATS {
    if let Some(bytes) = sdl::data(format.c_mime()).map_err(ClipboardError::Failed)? {
      return Ok(Some(Image { format, bytes: bytes.into() }));
    }
  }
  Ok(None)
}
