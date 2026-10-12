//! Microphone capture via the SDL audio subsystem.
//!
//! A session opens a recording device bound to an SDL audio stream that
//! converts to mono f32 at the requested sample rate, and its one output is
//! a `SampleRing` (ring.rs): SDL's audio thread drains the stream into it
//! from the stream's put callback, and every consumer reads the ring by a
//! cursor of its own - the session's own reader behind JS `read()`, and the
//! `MicrophoneTap`s a module or the speech recognizer take on any thread.
//! The ring holds a bounded window, so an unread session costs a fixed
//! amount of memory, not a growing one, and nobody has to pump for samples.
//!
//! Opening is the permission request. On Android a session starts `Pending`
//! while the RECORD_AUDIO prompt is up: the permission is asked for without
//! blocking (SDL's own recording open asks with a blocking callback and
//! would hold the opening thread behind the dialog), and the opener polls
//! `pump_microphone` on its own cadence, which performs the SDL open once
//! granted (`Ready`, or `Failed` with SDL's message) and closes the ring on
//! a denial (`Denied`, every reader wakes with `Closed`). The ring exists
//! from the open, so a tap taken while Pending hears from the grant. Every
//! other platform shows no prompt: the open runs at once and the session is
//! `Ready` before `open_microphone` returns, as is a test session.
//!
//! A test installs a clip as the microphone (`install_test_microphone`):
//! the device list is that one microphone, every open at the clip's rate is
//! a session with no SDL stream whose ring holds the clip from the open, and
//! a later install pushes the new clip into every open test session.

mod ring;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_int, c_void};
use std::mem::ManuallyDrop;
#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicI8, Ordering};
#[cfg(target_os = "android")]
use std::sync::Arc;
use std::sync::OnceLock;

pub(crate) use ring::SampleRing;
pub use ring::{MicRead, MicWait, MicrophoneTap, RING_SECONDS};
use sdl3::sys::audio::SDL_AudioStream;

use crate::sdl_utils;

pub struct MicrophoneInfo {
  pub id: u32,
  pub name: String,
}

/// The one device id the test microphone answers to.
const TEST_MICROPHONE_ID: u32 = 0;

/// The clip a test installed as the microphone.
struct TestClip {
  sample_rate: u32,
  samples: Vec<f32>,
}

#[derive(Clone)]
pub enum MicrophoneStatus {
  /// Waiting for the OS permission prompt (Android's RECORD_AUDIO).
  Pending,
  /// Capturing, mono f32 at `sample_rate`.
  Ready { sample_rate: u32 },
  /// The user refused the permission; the ring is closed.
  Denied,
  /// The device could not be opened once permission was granted; the ring
  /// is closed and the message explains what happened.
  Failed(String),
}

/// What a permission request has answered so far: nothing yet, granted,
/// or denied. The callback writes it, the pump reads it.
#[cfg(target_os = "android")]
const PERMISSION_PENDING: i8 = 0;
#[cfg(target_os = "android")]
const PERMISSION_GRANTED: i8 = 1;
#[cfg(target_os = "android")]
const PERMISSION_DENIED: i8 = -1;

/// Where a session's samples come from.
enum Source {
  /// A permission request on its way: the device to open once it is
  /// granted, and the answer the callback writes.
  #[cfg(target_os = "android")]
  Requested { device: Option<u32>, answer: Arc<AtomicI8> },
  /// A recording device: the stream SDL converts on, drained into the ring
  /// from its put callback, which holds `callback_ring`, a ring clone the
  /// close releases once the stream is destroyed.
  Device { stream: *mut SDL_AudioStream, callback_ring: *mut c_void },
  /// The installed test clip, pushed at the open and at every later install.
  Test,
  /// Nothing: the session was denied or failed, and its ring is closed.
  #[cfg(target_os = "android")]
  Ended,
}

struct Session {
  ring: SampleRing,
  /// The session's own reader, behind `read_microphone` (JS `read()`),
  /// hearing from the open.
  reader: MicrophoneTap,
  source: Source,
  status: MicrophoneStatus,
}

impl Drop for Session {
  // The one order that is safe with the callback: mark the ring closed
  // (readers wake with Closed), destroy the stream (SDL waits for a callback
  // in progress and fires none after), then let go of the ring clone the
  // callback held.
  fn drop(&mut self) {
    self.ring.close();
    if let Source::Device { stream, callback_ring } = self.source {
      sdl_utils::audio_stream_destroy(stream);
      drop(unsafe { SampleRing::from_raw(callback_ring) });
    }
  }
}

#[derive(Default)]
pub struct MicrophoneRegistry {
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
  test: RefCell<Option<TestClip>>,
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

/// SDL's answer to a permission request: runs once, on the activity's
/// thread, and hands the `Arc` the request lent it back. Must not panic: a
/// panic across the C boundary aborts.
#[cfg(target_os = "android")]
unsafe extern "C" fn on_permission(userdata: *mut c_void, _permission: *const std::ffi::c_char, granted: bool) {
  let answer = unsafe { Arc::from_raw(userdata as *const AtomicI8) };
  answer.store(if granted { PERMISSION_GRANTED } else { PERMISSION_DENIED }, Ordering::Release);
}

/// Ask Android for RECORD_AUDIO without waiting; the answer lands in the
/// returned cell (`PERMISSION_PENDING` until then).
#[cfg(target_os = "android")]
fn request_record_permission() -> Result<Arc<AtomicI8>, String> {
  let answer = Arc::new(AtomicI8::new(PERMISSION_PENDING));
  let lent = Arc::into_raw(answer.clone()) as *mut c_void;
  if !sdl_utils::request_android_permission(c"android.permission.RECORD_AUDIO", Some(on_permission), lent) {
    drop(unsafe { Arc::from_raw(lent as *const AtomicI8) });
    return Err(format!("could not request the microphone permission: {}", sdl_utils::sdl_error()));
  }
  Ok(answer)
}

/// Open the recording device bound to a stream drained into `ring`; the
/// capture runs when this returns.
fn open_device(device: Option<u32>, sample_rate: u32, ring: &SampleRing) -> Result<Source, String> {
  let callback_ring = ring.clone().into_raw();
  let stream = sdl_utils::audio_open_recording_stream(device, sample_rate, Some(on_captured), callback_ring);
  if stream.is_null() {
    drop(unsafe { SampleRing::from_raw(callback_ring) });
    return Err(format!("failed to open microphone: {}", sdl_utils::sdl_error()));
  }
  if !sdl_utils::audio_stream_resume(stream) {
    let err = format!("failed to start capture: {}", sdl_utils::sdl_error());
    sdl_utils::audio_stream_destroy(stream);
    drop(unsafe { SampleRing::from_raw(callback_ring) });
    return Err(err);
  }
  Ok(Source::Device { stream, callback_ring })
}

/// SDL's put callback on a recording stream: runs on SDL's audio thread with
/// the stream's lock held, after the device put data in. Drains the stream
/// into the ring (SDL allows the get inside the callback; the ring's lock is
/// a copy long). Must not panic: a panic across the C boundary aborts.
unsafe extern "C" fn on_captured(
  userdata: *mut c_void,
  stream: *mut SDL_AudioStream,
  _additional: c_int,
  _total: c_int,
) {
  let ring = ManuallyDrop::new(unsafe { SampleRing::from_raw(userdata) });
  let available = sdl_utils::audio_stream_available(stream);
  if available <= 0 {
    return;
  }
  ring.push_with(available as usize / 4, |scratch| sdl_utils::audio_stream_read_f32(stream, scratch).max(0) as usize);
}

impl crate::context::Context {
  pub fn list_microphones(&self) -> Vec<MicrophoneInfo> {
    if self.microphones.test.borrow().is_some() {
      return vec![MicrophoneInfo { id: TEST_MICROPHONE_ID, name: "Test microphone".to_string() }];
    }
    if let Err(e) = ensure_init() {
      log::warn!("[microphone] {e}");
      return Vec::new();
    }
    sdl_utils::audio_recording_ids()
      .into_iter()
      .map(|id| MicrophoneInfo { id, name: sdl_utils::audio_device_name(id) })
      .collect()
  }

  /// Open a microphone session delivering mono f32 samples at `sample_rate`.
  /// `device` picks an explicit id from `list_microphones`, otherwise the
  /// system default recording device. Returns the session id; the session
  /// is `Pending` while a permission prompt is up (Android), which the
  /// opener polls with `pump_microphone`, and `Ready` at once everywhere
  /// else. Err for what fails before any prompt: a rate of 0, a test clip at
  /// another rate, the audio subsystem, and on a platform without a prompt
  /// the device open itself.
  pub fn open_microphone(&self, device: Option<u32>, sample_rate: u32) -> Result<u64, String> {
    if sample_rate == 0 {
      return Err("sample rate must be above 0 Hz".to_string());
    }
    let is_test = match &*self.microphones.test.borrow() {
      Some(clip) if clip.sample_rate != sample_rate => {
        return Err(format!("the test microphone is a {} Hz clip, opened at {sample_rate} Hz", clip.sample_rate));
      }
      Some(_) => true,
      None => false,
    };
    if !is_test {
      ensure_init()?;
    }
    let ring = SampleRing::new(sample_rate, sdl_utils::ticks_ns());
    let reader = ring.reader();
    let (source, status) = if is_test {
      let test = self.microphones.test.borrow();
      ring.push(&test.as_ref().expect("test clip checked above").samples);
      (Source::Test, MicrophoneStatus::Ready { sample_rate })
    } else {
      #[cfg(target_os = "android")]
      {
        (Source::Requested { device, answer: request_record_permission()? }, MicrophoneStatus::Pending)
      }
      #[cfg(not(target_os = "android"))]
      {
        (open_device(device, sample_rate, &ring)?, MicrophoneStatus::Ready { sample_rate })
      }
    };
    let sid = {
      let mut next = self.microphones.next_id.borrow_mut();
      *next += 1;
      *next
    };
    self.microphones.sessions.borrow_mut().insert(sid, Session { ring, reader, source, status });
    Ok(sid)
  }

  pub fn microphone_status(&self, sid: u64) -> Option<MicrophoneStatus> {
    self.microphones.sessions.borrow().get(&sid).map(|s| s.status.clone())
  }

  /// Advance one session, for its opener polling on its own cadence: a
  /// permission answer takes the session out of `Pending`, a grant by the
  /// SDL open (`Ready`, or `Failed` when the device refuses), a denial by
  /// closing the ring (`Denied`). The session stays registered either way,
  /// until its owner closes it. Returns the status after the step; None for
  /// a closed session.
  pub fn pump_microphone(&self, sid: u64) -> Option<MicrophoneStatus> {
    let mut sessions = self.microphones.sessions.borrow_mut();
    let session = sessions.get_mut(&sid)?;
    #[cfg(target_os = "android")]
    if let Source::Requested { device, answer } = &session.source {
      let (device, sample_rate) = (*device, session.reader.sample_rate());
      let status = match answer.load(Ordering::Acquire) {
        PERMISSION_PENDING => return Some(MicrophoneStatus::Pending),
        PERMISSION_GRANTED => match open_device(device, sample_rate, &session.ring) {
          Ok(source) => {
            session.source = source;
            MicrophoneStatus::Ready { sample_rate }
          }
          Err(message) => {
            log::warn!("[microphone] {message}");
            MicrophoneStatus::Failed(message)
          }
        },
        _ => {
          log::warn!("[microphone] permission denied");
          MicrophoneStatus::Denied
        }
      };
      if !matches!(status, MicrophoneStatus::Ready { .. }) {
        session.ring.close();
        session.source = Source::Ended;
      }
      session.status = status;
    }
    Some(session.status.clone())
  }

  /// Drain the samples captured since the last read (possibly empty) through
  /// the session's own reader. A reader this far behind (`RING_SECONDS`
  /// unread) lost the oldest samples; that is logged, not returned.
  pub fn read_microphone(&self, sid: u64) -> Result<Vec<f32>, String> {
    let mut sessions = self.microphones.sessions.borrow_mut();
    let session = sessions.get_mut(&sid).ok_or_else(|| "microphone closed".to_string())?;
    let mut samples = Vec::with_capacity(session.reader.available());
    let read = session.reader.read_into(&mut samples)?;
    if read.dropped > 0 {
      log::warn!("[microphone] session {sid}: {} samples lost, read less than once per {RING_SECONDS} s", read.dropped);
    }
    Ok(samples)
  }

  /// A new reader on the session, hearing from now on, for any thread: a
  /// module's worker, the speech recognizer. Err for a session that is not
  /// open.
  pub fn microphone_tap(&self, sid: u64) -> Result<MicrophoneTap, String> {
    let sessions = self.microphones.sessions.borrow();
    let session = sessions.get(&sid).ok_or_else(|| "microphone closed".to_string())?;
    Ok(session.ring.reader())
  }

  /// Close the session and release the device. Every reader on it wakes
  /// with `Closed`.
  pub fn close_microphone(&self, sid: u64) {
    self.microphones.sessions.borrow_mut().remove(&sid);
  }

  /// Make a clip the microphone, for tests: the device list is that one
  /// microphone, every open at the clip's rate from now on is a session
  /// holding the clip from the open, with no SDL stream, and every open test
  /// session hears the clip now. Err for a rate of 0 or a rate that differs
  /// from an open test session's.
  pub fn install_test_microphone(&self, sample_rate: u32, samples: Vec<f32>) -> Result<(), String> {
    if sample_rate == 0 {
      return Err("sample rate must be above 0 Hz".to_string());
    }
    let sessions = self.microphones.sessions.borrow();
    let open: Vec<&Session> = sessions.values().filter(|s| matches!(s.source, Source::Test)).collect();
    if let Some(other) = open.iter().find(|s| s.reader.sample_rate() != sample_rate) {
      return Err(format!(
        "an open test microphone hears at {} Hz; install a clip at that rate, not {sample_rate} Hz",
        other.reader.sample_rate()
      ));
    }
    for session in open {
      session.ring.push(&samples);
    }
    *self.microphones.test.borrow_mut() = Some(TestClip { sample_rate, samples });
    Ok(())
  }
}
