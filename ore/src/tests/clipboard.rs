use std::sync::{mpsc, Arc};

use crate::clipboard::{ClipboardError, Content, Image, ImageFormat, Responder, Session, PNG_SIGNATURE};

/// A record-only session: the engine's own clipboard, never the OS's.
fn recording() -> Arc<Session> {
  let session = Session::new();
  session.set_record_only();
  session
}

/// Bytes that pass the PNG check: the signature and a few more.
fn png() -> Arc<[u8]> {
  let mut bytes = PNG_SIGNATURE.to_vec();
  bytes.extend_from_slice(b"rest");
  bytes.into()
}

/// A responder and the receiver its one answer lands in.
fn answer<T: Send + 'static>() -> (Responder<T>, mpsc::Receiver<Result<T, ClipboardError>>) {
  let (tx, rx) = mpsc::channel();
  (
    Box::new(move |result| {
      tx.send(result).expect("send result");
    }),
    rx,
  )
}

fn write_text(session: &Arc<Session>, text: &str) -> Result<(), ClipboardError> {
  let (respond, rx) = answer();
  session.write_text(text.to_string(), respond);
  rx.try_recv().expect("answered at once")
}

fn read_text(session: &Session) -> Result<String, ClipboardError> {
  let (respond, rx) = answer();
  session.read_text(respond);
  rx.try_recv().expect("answered at once")
}

fn write_image(session: &Arc<Session>, bytes: Arc<[u8]>) -> Result<(), ClipboardError> {
  let (respond, rx) = answer();
  session.write_image(bytes, respond);
  rx.try_recv().expect("answered at once")
}

fn read_image(session: &Session) -> Result<Option<Image>, ClipboardError> {
  let (respond, rx) = answer();
  session.read_image(respond);
  rx.try_recv().expect("answered at once")
}

#[test]
fn text_reads_back_what_was_written() {
  let session = recording();
  assert_eq!(read_text(&session), Ok(String::new()));
  assert_eq!(write_text(&session, "hello"), Ok(()));
  assert_eq!(read_text(&session), Ok("hello".to_string()));
  assert_eq!(session.held(), Some(Content::Text("hello".to_string())));
}

#[test]
fn an_image_reads_back_what_was_written() {
  let session = recording();
  assert_eq!(read_image(&session), Ok(None));
  assert_eq!(write_image(&session, png()), Ok(()));
  let image = Image { format: ImageFormat::Png, bytes: png() };
  assert_eq!(read_image(&session), Ok(Some(image.clone())));
  assert_eq!(session.held(), Some(Content::Image(image)));
}

#[test]
fn an_image_write_replaces_the_text() {
  let session = recording();
  write_text(&session, "hello").expect("text written");
  write_image(&session, png()).expect("image written");
  assert_eq!(read_text(&session), Ok(String::new()));
}

#[test]
fn a_text_write_replaces_the_image() {
  let session = recording();
  write_image(&session, png()).expect("image written");
  write_text(&session, "hello").expect("text written");
  assert_eq!(read_image(&session), Ok(None));
}

#[test]
fn an_empty_text_clears_the_clipboard() {
  let session = recording();
  write_image(&session, png()).expect("image written");
  assert_eq!(write_text(&session, ""), Ok(()));
  assert_eq!(session.held(), None);
  assert_eq!(read_text(&session), Ok(String::new()));
}

#[test]
fn bytes_that_are_not_a_png_are_refused_and_change_nothing() {
  let session = recording();
  write_text(&session, "kept").expect("text written");
  assert_eq!(write_image(&session, Arc::from(&b"GIF89a"[..])), Err(ClipboardError::NotPng));
  assert_eq!(write_image(&session, Arc::from(&PNG_SIGNATURE[..PNG_SIGNATURE.len() - 1])), Err(ClipboardError::NotPng));
  assert_eq!(session.held(), Some(Content::Text("kept".to_string())));
}

#[test]
fn what_a_test_puts_is_read_whatever_its_format() {
  let session = recording();
  let png = Image { format: ImageFormat::Png, bytes: png() };
  session.put(Some(Content::Image(png.clone())));
  assert_eq!(read_image(&session), Ok(Some(png)));
  let bmp = Image { format: ImageFormat::Bmp, bytes: Arc::from(&b"BM"[..]) };
  session.put(Some(Content::Image(bmp.clone())));
  assert_eq!(read_image(&session), Ok(Some(bmp)));
  session.put(Some(Content::Text("pasted".to_string())));
  assert_eq!(read_text(&session), Ok("pasted".to_string()));
  session.put(Some(Content::Text(String::new())));
  assert_eq!(session.held(), None);
  session.put(None);
  assert_eq!(read_image(&session), Ok(None));
}

#[test]
fn the_formats_round_trip_their_mime_types() {
  for format in [ImageFormat::Png, ImageFormat::Bmp] {
    assert_eq!(ImageFormat::from_mime(format.mime()), Some(format));
  }
  assert_eq!(ImageFormat::from_mime("image/jpeg"), None);
}

// The branches that answer before the bridge: no SDL is reached, so no
// video subsystem is needed.

#[test]
fn a_session_on_the_os_refuses_bytes_that_are_not_a_png_first() {
  let session = Session::new();
  assert_eq!(write_image(&session, Arc::from(&b"not a png"[..])), Err(ClipboardError::NotPng));
  assert_eq!(session.held(), None);
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[test]
fn a_platform_without_an_image_clipboard_refuses_an_image() {
  let session = Session::new();
  assert_eq!(write_image(&session, png()), Err(ClipboardError::Unavailable));
  assert_eq!(read_image(&session), Err(ClipboardError::Unavailable));
  assert_eq!(session.held(), None);
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[test]
fn the_desktops_have_an_image_clipboard() {
  assert!(crate::clipboard::image_available());
}

#[test]
fn a_text_with_a_nul_character_is_refused_as_on_the_os() {
  let session = recording();
  write_text(&session, "kept").expect("text written");
  assert_eq!(
    write_text(&session, "a\0b"),
    Err(ClipboardError::Failed("the text contains a NUL character".to_string()))
  );
  assert_eq!(session.held(), Some(Content::Text("kept".to_string())));
  let on_the_os = Session::new();
  assert_eq!(
    write_text(&on_the_os, "a\0b"),
    Err(ClipboardError::Failed("the text contains a NUL character".to_string()))
  );
}

// Without SDL initialized the bridge runs the call at once on this thread
// (SDL_RunOnMainThread with no event subsystem), and the video check fails
// before SDL is reached.
#[test]
fn a_failed_write_on_the_os_leaves_the_session_claiming_nothing() {
  let session = Session::new();
  session.put(Some(Content::Text("earlier".to_string())));
  let result = write_text(&session, "x");
  assert!(matches!(result, Err(ClipboardError::Failed(_))), "expected a failure, got {result:?}");
  assert_eq!(session.held(), None);
}
