//! Microphone capture via the SDL audio subsystem.
//!
//! A session opens a recording device bound to an SDL audio stream that
//! converts to mono f32 at the requested sample rate. SDL buffers captured
//! audio inside the stream until it is read, so there is no per-frame pump:
//! consumers drain with `read_microphone` whenever they want. An open session
//! that is never read accumulates memory, so callers should poll regularly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use sdl3::sys::audio::SDL_AudioStream;

use crate::sdl_utils;

pub struct MicrophoneInfo {
  pub id: u32,
  pub name: String,
}

struct Session {
  stream: *mut SDL_AudioStream,
}

#[derive(Default)]
pub struct MicrophoneRegistry {
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
}

/// Lazy one-time SDL audio subsystem init (first list/open call).
fn ensure_init() -> Result<(), String> {
  static INIT: OnceLock<bool> = OnceLock::new();
  let ok = *INIT.get_or_init(sdl_utils::audio_subsystem_init);
  if ok {
    Ok(())
  } else {
    Err(format!("audio subsystem init failed: {}", sdl_utils::sdl_error()))
  }
}

pub fn list_microphones() -> Vec<MicrophoneInfo> {
  if let Err(e) = ensure_init() {
    log::warn!("[microphone] {e}");
    return Vec::new();
  }
  sdl_utils::audio_recording_ids()
    .into_iter()
    .map(|id| MicrophoneInfo { id, name: sdl_utils::audio_device_name(id) })
    .collect()
}

impl crate::context::Context {
  /// Open a microphone session delivering mono f32 samples at `sample_rate`.
  /// `device` picks an explicit id from `list_microphones`, otherwise the
  /// system default recording device. Capture starts immediately; returns the
  /// session id.
  pub fn open_microphone(&self, device: Option<u32>, sample_rate: u32) -> Result<u64, String> {
    ensure_init()?;
    let stream = sdl_utils::audio_open_recording_stream(device, sample_rate);
    if stream.is_null() {
      return Err(format!("failed to open microphone: {}", sdl_utils::sdl_error()));
    }
    if !sdl_utils::audio_stream_resume(stream) {
      let err = format!("failed to start capture: {}", sdl_utils::sdl_error());
      sdl_utils::audio_stream_destroy(stream);
      return Err(err);
    }
    let sid = {
      let mut next = self.microphones.next_id.borrow_mut();
      *next += 1;
      *next
    };
    self.microphones.sessions.borrow_mut().insert(sid, Session { stream });
    Ok(sid)
  }

  /// Drain the samples captured since the last read (possibly empty).
  pub fn read_microphone(&self, sid: u64) -> Result<Vec<f32>, String> {
    let sessions = self.microphones.sessions.borrow();
    let session = sessions.get(&sid).ok_or_else(|| "microphone closed".to_string())?;
    let available = sdl_utils::audio_stream_available(session.stream);
    if available <= 0 {
      return Ok(Vec::new());
    }
    let mut samples = vec![0f32; available as usize / 4];
    let read = sdl_utils::audio_stream_read_f32(session.stream, &mut samples);
    if read < 0 {
      return Err(format!("microphone read failed: {}", sdl_utils::sdl_error()));
    }
    samples.truncate(read as usize);
    Ok(samples)
  }

  /// Close the session and release the device.
  pub fn close_microphone(&self, sid: u64) {
    if let Some(session) = self.microphones.sessions.borrow_mut().remove(&sid) {
      sdl_utils::audio_stream_destroy(session.stream);
    }
  }

  /// Release every open microphone. Called between engine runs so a reloaded
  /// app never inherits (or leaks) a live capture device.
  pub fn close_all_microphones(&self) {
    for (_, session) in self.microphones.sessions.borrow_mut().drain() {
      sdl_utils::audio_stream_destroy(session.stream);
    }
  }
}
