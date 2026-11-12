//! The audio device facility: the playback device pulling a render
//! callback (`Playback`), the unbound converter stream a voice resamples
//! through (`Converter`), the recording device pushing a capture callback
//! (`Recording`, with `recording_devices`), and on Android the resume check
//! that reopens a playback device SDL's restart left dead.
//!
//! Every device is SDL's audio subsystem, reached through `sdl3::sys`: the
//! sdl3 crate's safe `AudioSubsystem` is !Send and main-thread-bound, while
//! the mixer and capture sessions live on the UI thread. SDL's stream calls
//! are thread-safe, so every type here is `Send` and callable from any one
//! thread at a time. The callbacks run on SDL's audio thread; a panic in one
//! is caught and logged, never unwound into SDL.

use std::ffi::{c_int, c_void, CStr};
use std::marker::PhantomData;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use sdl3::sys::audio::{
  SDL_AudioDeviceID, SDL_AudioSpec, SDL_AudioStream, SDL_ClearAudioStream, SDL_CreateAudioStream,
  SDL_DestroyAudioStream, SDL_FlushAudioStream, SDL_GetAudioDeviceFormat, SDL_GetAudioDeviceName,
  SDL_GetAudioRecordingDevices, SDL_GetAudioStreamAvailable, SDL_GetAudioStreamData, SDL_GetAudioStreamQueued,
  SDL_OpenAudioDeviceStream, SDL_PutAudioStreamData, SDL_ResumeAudioStreamDevice, SDL_SetAudioStreamFrequencyRatio,
  SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, SDL_AUDIO_DEVICE_DEFAULT_RECORDING, SDL_AUDIO_F32,
};
use sdl3::sys::init::{SDL_InitSubSystem, SDL_INIT_AUDIO};
use sdl3::sys::stdinc::SDL_free;

use crate::lifecycle::Lifecycle;

/// The device rate assumed when SDL cannot say (it converts to the real
/// one either way).
const FALLBACK_DEVICE_RATE: u32 = 48_000;
/// Bytes in one f32 sample: every stream here carries f32.
const SAMPLE_BYTES: usize = std::mem::size_of::<f32>();
/// How long the device callback has after a return to the foreground to
/// pull before the device stream counts as dead and is reopened: many
/// device buffers (AAudio's run 10 to 20 ms), so a slow restart is not
/// taken for a dead one.
const RESUME_PULL_WINDOW: Duration = Duration::from_millis(500);

// The open playback device, for the lifecycle hooks below: they run on the
// main thread, away from its owner. Set at open, taken at drop.
static OPEN: Mutex<Option<Arc<Inner>>> = Mutex::new(None);
// Bumped on every background and foreground transition, so a pending
// resume check knows the app left again before it ran.
static TRANSITIONS: AtomicU64 = AtomicU64::new(0);

/// SDL's audio subsystem, initialized once (first open or list); the error
/// is the one the first attempt got, kept for every later caller.
fn ensure_init() -> Result<(), String> {
  static INIT: OnceLock<Result<(), String>> = OnceLock::new();
  INIT
    .get_or_init(|| {
      // SAFETY: a plain subsystem init with no pointers, run once through
      // the OnceLock; SDL refcounts the subsystem against the host's own
      // init.
      if unsafe { SDL_InitSubSystem(SDL_INIT_AUDIO) } {
        Ok(())
      } else {
        Err(format!("audio subsystem init failed: {}", crate::sdl_error()))
      }
    })
    .clone()
}

/// An f32 spec at `rate` and `channels`.
fn spec(rate: u32, channels: u16) -> SDL_AudioSpec {
  SDL_AudioSpec { format: SDL_AUDIO_F32, channels: channels as c_int, freq: rate as c_int }
}

/// The default playback device's own format: (sample rate, the device's
/// buffer size in frames), the rate `Playback::open` opens at. When SDL
/// cannot query the device, `FALLBACK_DEVICE_RATE` and 0 frames (SDL
/// converts to the real rate either way). Initializes the audio subsystem;
/// the error is its failure.
pub fn default_playback_format() -> Result<(u32, u32), String> {
  ensure_init()?;
  let mut spec = spec(0, 0);
  let mut frames: c_int = 0;
  // SAFETY: both out-pointers are valid locals for the call.
  let known = unsafe { SDL_GetAudioDeviceFormat(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &mut spec, &mut frames) };
  if known && spec.freq > 0 {
    Ok((spec.freq as u32, frames.max(0) as u32))
  } else {
    Ok((FALLBACK_DEVICE_RATE, 0))
  }
}

/// An SDL audio stream pointer that may cross threads: only ever used
/// through SDL's thread-safe stream calls.
struct StreamPtr(*mut SDL_AudioStream);

// SAFETY: SDL's audio stream calls are thread-safe (each takes the stream's
// own lock); the pointer is only used through them, under `Inner::stream`.
unsafe impl Send for StreamPtr {}

type Render = Box<dyn FnMut(&mut Pull<'_>) + Send>;

/// What the playback callback and its owner share. Leaked into the
/// callback's userdata for the process lifetime.
struct Inner {
  device_rate: u32,
  device_frames: u32,
  channels: u16,
  /// The device stream, for the owner's queue reads and the reopen; set at
  /// open and replaced by `reopen`, which holds the lock across the swap.
  /// The callback never takes it: SDL hands it the stream.
  stream: Mutex<StreamPtr>,
  /// The render callback. The device callback try-locks it: a miss is a
  /// reentry SDL does not do.
  render: Mutex<Render>,
  /// Callback runs so far: the resume check's sign of a live device.
  pulls: AtomicU64,
}

impl Inner {
  /// Open a device stream on the default playback device pulling into
  /// this state; null on failure. The stream starts paused.
  fn open_stream(self: &Arc<Self>) -> *mut SDL_AudioStream {
    // The callback's reference was leaked at the first open and is alive
    // for the process: every stream takes the same userdata.
    let userdata = Arc::as_ptr(self) as *mut c_void;
    let spec = spec(self.device_rate, self.channels);
    // SAFETY: the spec is a valid local; `userdata` points at an Inner that
    // is never freed while a stream on it exists (see `Playback::open`), and
    // `on_pull` reads it as exactly that.
    unsafe { SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, Some(on_pull), userdata) }
  }

  /// Destroy the device stream and open a new one on the default playback
  /// device, pulling into this same state. Destroying the only stream on
  /// the device closes SDL's physical device and ends its device thread. A
  /// device closed since (its `Playback` dropped) is left closed.
  fn reopen(self: &Arc<Self>) {
    let mut stream = self.stream.lock().expect("audio stream lock poisoned");
    let open = OPEN.lock().expect("audio open device lock poisoned").as_ref().is_some_and(|o| Arc::ptr_eq(o, self));
    if !open {
      return;
    }
    // SAFETY: the stream is ours (or null, which SDL ignores), and no one
    // else uses it while the lock is held; the callback SDL may be running
    // finishes before the destroy returns.
    unsafe { SDL_DestroyAudioStream(stream.0) };
    stream.0 = self.open_stream();
    if stream.0.is_null() {
      log::warn!("[ore::audio] Reopen playback device: {}", crate::sdl_error());
      return;
    }
    // SAFETY: a live stream, opened just above.
    if !unsafe { SDL_ResumeAudioStreamDevice(stream.0) } {
      log::warn!("[ore::audio] Resume reopened device stream: {}", crate::sdl_error());
    }
  }
}

/// The default playback device, opened at its own rate, pulling interleaved
/// f32 at the opened channel count from a render callback on SDL's audio
/// thread. A handle for the owner's thread; the device closes when it drops.
pub struct Playback {
  inner: Arc<Inner>,
}

impl Playback {
  /// Open the default playback device at `default_playback_format`'s rate
  /// and start pulling: `render` runs on SDL's audio thread each time the
  /// device wants more than the stream holds, and puts what it rendered
  /// (`Pull`). It must not block. Kept for the process even after the
  /// `Playback` drops (SDL may still be inside it while the device closes),
  /// as the device usually is.
  pub fn open(channels: u16, render: impl FnMut(&mut Pull<'_>) + Send + 'static) -> Result<Playback, String> {
    let (device_rate, device_frames) = default_playback_format()?;
    let inner = Arc::new(Inner {
      device_rate,
      device_frames,
      channels,
      stream: Mutex::new(StreamPtr(std::ptr::null_mut())),
      render: Mutex::new(Box::new(render)),
      pulls: AtomicU64::new(0),
    });
    let stream = inner.open_stream();
    if stream.is_null() {
      return Err(format!("failed to open audio device: {}", crate::sdl_error()));
    }
    // The callback's reference: never released, like the device.
    let _ = Arc::into_raw(Arc::clone(&inner));
    inner.stream.lock().expect("audio stream lock poisoned").0 = stream;
    // SAFETY: a live stream, opened just above.
    if !unsafe { SDL_ResumeAudioStreamDevice(stream) } {
      log::warn!("[ore::audio] Resume device stream: {}", crate::sdl_error());
    }
    *OPEN.lock().expect("audio open device lock poisoned") = Some(Arc::clone(&inner));
    Ok(Playback { inner })
  }

  /// The rate the device was opened at, Hz.
  pub fn device_rate(&self) -> u32 {
    self.inner.device_rate
  }

  /// The device's buffer size in frames, as SDL reported it at open (0 when
  /// it could not say).
  pub fn device_frames(&self) -> u32 {
    self.inner.device_frames
  }

  /// Frames the device stream holds, rendered and not yet consumed by the
  /// device (the stream is at the device's own rate, so nothing waits in
  /// conversion). 0 while the stream is closed.
  pub fn queued_frames(&self) -> u64 {
    let stream = self.inner.stream.lock().expect("audio stream lock poisoned");
    if stream.0.is_null() {
      return 0;
    }
    // SAFETY: a live stream, kept alive by the lock held.
    let bytes = unsafe { SDL_GetAudioStreamQueued(stream.0) };
    bytes.max(0) as u64 / (SAMPLE_BYTES as u64 * self.inner.channels as u64)
  }
}

impl Drop for Playback {
  fn drop(&mut self) {
    // A dropped device must not leave SDL pulling into a render callback
    // its owner thinks gone; the callback itself stays alive (leaked).
    let mut open = OPEN.lock().expect("audio open device lock poisoned");
    if open.as_ref().is_some_and(|o| Arc::ptr_eq(o, &self.inner)) {
      open.take();
    }
    drop(open);
    let mut stream = self.inner.stream.lock().expect("audio stream lock poisoned");
    // SAFETY: the stream is ours (or null, which SDL ignores); SDL waits for
    // a callback in progress and fires none after.
    unsafe { SDL_DestroyAudioStream(stream.0) };
    stream.0 = std::ptr::null_mut();
  }
}

/// One pull of the playback device: how many frames it asks for, how many
/// earlier pulls left unheard, and where to put the rendered frames.
pub struct Pull<'a> {
  stream: *mut SDL_AudioStream,
  frames: usize,
  lead: u64,
  channels: u16,
  _callback: PhantomData<&'a mut SDL_AudioStream>,
}

impl Pull<'_> {
  /// Frames the device asks for in this pull.
  pub fn frames(&self) -> usize {
    self.frames
  }

  /// Frames of earlier pulls the stream still holds, rendered and unheard
  /// (read once, before this pull put anything).
  pub fn lead_frames(&self) -> u64 {
    self.lead
  }

  /// Queue interleaved f32 samples at the opened channel count for the
  /// device (SDL buffers them; a failed put is dropped).
  pub fn put(&mut self, samples: &[f32]) {
    debug_assert_eq!(samples.len() % self.channels as usize, 0);
    // SAFETY: the stream is the one SDL is calling back for, alive for the
    // callback, and `samples` is valid for its length in bytes.
    unsafe {
      SDL_PutAudioStreamData(self.stream, samples.as_ptr() as *const c_void, (samples.len() * SAMPLE_BYTES) as c_int)
    };
  }
}

/// SDL's get callback on the playback stream: runs on SDL's audio thread,
/// with the stream's lock held, each time the device wants more than the
/// stream holds; SDL allows the put right here.
unsafe extern "C" fn on_pull(userdata: *mut c_void, stream: *mut SDL_AudioStream, additional: c_int, _total: c_int) {
  // SAFETY: the userdata is the leaked Arc<Inner> of `Playback::open`,
  // alive for the process.
  let inner = unsafe { &*(userdata as *const Inner) };
  inner.pulls.fetch_add(1, Ordering::AcqRel);
  let frame_bytes = SAMPLE_BYTES * inner.channels as usize;
  // SAFETY: the stream SDL is calling back for, alive for the call.
  let queued = unsafe { SDL_GetAudioStreamQueued(stream) };
  let mut pull = Pull {
    stream,
    frames: additional.max(0) as usize / frame_bytes,
    lead: queued.max(0) as u64 / frame_bytes as u64,
    channels: inner.channels,
    _callback: PhantomData,
  };
  let Ok(mut render) = inner.render.try_lock() else {
    // Only this callback ever takes it; a miss is a reentry SDL does not do.
    return;
  };
  if std::panic::catch_unwind(AssertUnwindSafe(|| render(&mut pull))).is_err() {
    log::error!("[ore::audio] The playback render callback panicked");
  }
}

/// The lifecycle hook ore's watch runs ahead of the host's (see
/// `crate::platform::Platform::on_lifecycle`): the Android resume check.
pub(crate) fn on_lifecycle(lifecycle: &Lifecycle) {
  if !cfg!(target_os = "android") {
    return;
  }
  match lifecycle {
    Lifecycle::DidBackground => backgrounded(),
    Lifecycle::DidForeground => foregrounded(),
    Lifecycle::WillBackground(_) => {}
  }
}

/// The app went to the background (Android). Cancels a pending resume
/// check.
fn backgrounded() {
  TRANSITIONS.fetch_add(1, Ordering::AcqRel);
}

/// The app came back to the foreground (Android): reopen the device stream
/// if it does not pull again within `RESUME_PULL_WINDOW`. Works around SDL's
/// AAudio backend, which ignores a failed restart of a stream Android
/// disconnected while it was paused and then never pulls again
/// (okf/upstream/sdl-android-aaudio-resume-disconnected-never-recovers.md).
/// Runs on the main thread, inside SDL's event watch: the check waits on a
/// thread of its own.
fn foregrounded() {
  let transition = TRANSITIONS.fetch_add(1, Ordering::AcqRel) + 1;
  let Some(inner) = OPEN.lock().expect("audio open device lock poisoned").clone() else {
    return;
  };
  let pulls = inner.pulls.load(Ordering::Acquire);
  let spawned = std::thread::Builder::new().name("sol-audio-resume".into()).spawn(move || {
    std::thread::sleep(RESUME_PULL_WINDOW);
    if TRANSITIONS.load(Ordering::Acquire) != transition || inner.pulls.load(Ordering::Acquire) != pulls {
      return;
    }
    log::warn!("[ore::audio] Playback device did not restart after resume; reopening it");
    inner.reopen();
  });
  if let Err(e) = spawned {
    log::warn!("[ore::audio] Spawn resume check: {e}");
  }
}

/// An unbound SDL audio stream converting interleaved f32 from one (rate,
/// channels) to another: the resampler and channel converter a voice
/// renders through. Not tied to a device. Created and destroyed on one
/// thread, used on another (all thread-safe on SDL's side, one user at a
/// time through `&mut`).
pub struct Converter {
  stream: *mut SDL_AudioStream,
  dst_channels: u16,
}

// SAFETY: SDL's audio stream calls are thread-safe, and the stream is owned
// by this value alone.
unsafe impl Send for Converter {}

impl Converter {
  /// A stream converting interleaved f32 from `src` to `dst` (each a
  /// (sample rate, channels) pair). Needs no open device.
  pub fn new(src: (u32, u16), dst: (u32, u16)) -> Result<Converter, String> {
    let (src, dst_spec) = (spec(src.0, src.1), spec(dst.0, dst.1));
    // SAFETY: both specs are valid locals for the call.
    let stream = unsafe { SDL_CreateAudioStream(&src, &dst_spec) };
    if stream.is_null() {
      return Err(format!("failed to create audio converter: {}", crate::sdl_error()));
    }
    Ok(Converter { stream, dst_channels: dst.1 })
  }

  /// Queue interleaved source samples (non-blocking; SDL buffers).
  pub fn put(&mut self, samples: &[f32]) {
    // SAFETY: the stream is ours and live; `samples` is valid for its
    // length in bytes.
    unsafe {
      SDL_PutAudioStreamData(self.stream, samples.as_ptr() as *const c_void, (samples.len() * SAMPLE_BYTES) as c_int)
    };
  }

  /// No more input is coming: convert everything queued so it can be read.
  pub fn flush(&mut self) {
    // SAFETY: the stream is ours and live.
    unsafe { SDL_FlushAudioStream(self.stream) };
  }

  /// Destination frames buffered, already converted (an estimate while
  /// resampling, exact after a flush).
  pub fn available(&self) -> usize {
    // SAFETY: the stream is ours and live.
    let bytes = unsafe { SDL_GetAudioStreamAvailable(self.stream) };
    bytes.max(0) as usize / (SAMPLE_BYTES * self.dst_channels as usize)
  }

  /// Drain converted destination frames into `out` (non-blocking), whole
  /// frames only; the frames written, 0 on error.
  pub fn get(&mut self, out: &mut [f32]) -> usize {
    let channels = self.dst_channels as usize;
    let whole = out.len() / channels * channels;
    let out = &mut out[..whole];
    // SAFETY: the stream is ours and live; `out` is valid for its length in
    // bytes.
    let bytes = unsafe {
      SDL_GetAudioStreamData(self.stream, out.as_mut_ptr() as *mut c_void, (out.len() * SAMPLE_BYTES) as c_int)
    };
    bytes.max(0) as usize / (SAMPLE_BYTES * channels)
  }

  /// The rate input is consumed at relative to the stream's own conversion
  /// (1.0 = as created; the range is 0.01 to 100).
  pub fn set_ratio(&mut self, ratio: f32) {
    // SAFETY: the stream is ours and live.
    unsafe { SDL_SetAudioStreamFrequencyRatio(self.stream, ratio) };
  }

  /// Drop everything queued on the stream (input and converted output).
  pub fn clear(&mut self) {
    // SAFETY: the stream is ours and live.
    unsafe { SDL_ClearAudioStream(self.stream) };
  }
}

impl Drop for Converter {
  fn drop(&mut self) {
    // SAFETY: the stream is ours, destroyed once.
    unsafe { SDL_DestroyAudioStream(self.stream) };
  }
}

/// An audio recording device as SDL lists it.
pub struct MicrophoneInfo {
  pub id: u32,
  pub name: String,
}

/// The recording devices SDL knows. Initializes the audio subsystem; on a
/// failure, warns and lists none.
pub fn recording_devices() -> Vec<MicrophoneInfo> {
  if let Err(e) = ensure_init() {
    log::warn!("[ore::audio] {e}");
    return Vec::new();
  }
  let mut count: c_int = 0;
  // SAFETY: `count` is a valid local; the array SDL returns is ours to free.
  let ids = unsafe { SDL_GetAudioRecordingDevices(&mut count) };
  if ids.is_null() {
    return Vec::new();
  }
  let devices = (0..count.max(0) as usize)
    .map(|i| {
      // SAFETY: `ids` holds `count` ids.
      let id = unsafe { *ids.add(i) };
      MicrophoneInfo { id: id.0, name: device_name(id) }
    })
    .collect();
  // SAFETY: SDL allocated the array; freed once, after the last read.
  unsafe { SDL_free(ids as *mut c_void) };
  devices
}

/// The name of an audio device, empty when SDL does not know it.
fn device_name(id: SDL_AudioDeviceID) -> String {
  // SAFETY: any id is accepted; SDL returns null for an unknown one.
  let name = unsafe { SDL_GetAudioDeviceName(id) };
  if name.is_null() {
    return String::new();
  }
  // SAFETY: a non-null name is a NUL-terminated string SDL keeps alive at
  // least until the device list changes; copied out at once.
  unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
}

type Capture = Box<dyn FnMut(&mut Captured<'_>) + Send>;

/// A recording device bound to a stream delivering mono f32 at the opened
/// rate (SDL converts from the device format) to a capture callback. The
/// device closes when it drops.
pub struct Recording {
  stream: *mut SDL_AudioStream,
  /// The boxed capture callback, the stream's userdata; reclaimed at drop,
  /// after the stream is gone.
  callback: *mut Capture,
}

// SAFETY: SDL's audio stream calls are thread-safe; the stream and the
// callback box are owned by this value alone (the callback itself is Send).
unsafe impl Send for Recording {}

impl Recording {
  /// Open a recording device (None: the system default) and start the
  /// capture. `on_captured` runs on SDL's audio thread, with the stream's
  /// lock held, each time the device put data in, and may read the stream
  /// right there (`Captured`). It must not block.
  pub fn open(
    device: Option<u32>,
    sample_rate: u32,
    on_captured: impl FnMut(&mut Captured<'_>) + Send + 'static,
  ) -> Result<Recording, String> {
    ensure_init()?;
    let callback: *mut Capture = Box::into_raw(Box::new(Box::new(on_captured)));
    let spec = spec(sample_rate, 1);
    let devid = device.map(SDL_AudioDeviceID).unwrap_or(SDL_AUDIO_DEVICE_DEFAULT_RECORDING);
    // SAFETY: the spec is a valid local; `callback` is an owned allocation
    // that outlives the stream (reclaimed only after the destroy), and
    // `on_captured_c` reads it as exactly that type.
    let stream = unsafe { SDL_OpenAudioDeviceStream(devid, &spec, Some(on_captured_c), callback as *mut c_void) };
    if stream.is_null() {
      let err = format!("failed to open microphone: {}", crate::sdl_error());
      // SAFETY: no stream took the pointer; reclaimed once.
      drop(unsafe { Box::from_raw(callback) });
      return Err(err);
    }
    // SAFETY: a live stream, opened just above.
    if !unsafe { SDL_ResumeAudioStreamDevice(stream) } {
      let err = format!("failed to start capture: {}", crate::sdl_error());
      // SAFETY: the stream is ours; after the destroy no callback runs, so
      // the box is reclaimed once, by us.
      unsafe { SDL_DestroyAudioStream(stream) };
      drop(unsafe { Box::from_raw(callback) });
      return Err(err);
    }
    Ok(Recording { stream, callback })
  }
}

impl Drop for Recording {
  // The one order that is safe with the callback: destroy the stream first
  // (SDL waits for a callback in progress and fires none after), then drop
  // the boxed callback. That is what lets the callback own what it writes
  // into (a ring clone, say).
  fn drop(&mut self) {
    // SAFETY: the stream is ours, destroyed once.
    unsafe { SDL_DestroyAudioStream(self.stream) };
    // SAFETY: the box from `open`; no callback runs any more.
    drop(unsafe { Box::from_raw(self.callback) });
  }
}

/// The recording stream inside a capture callback: what the device put in,
/// converted to mono f32 at the opened rate.
pub struct Captured<'a> {
  stream: *mut SDL_AudioStream,
  _callback: PhantomData<&'a mut SDL_AudioStream>,
}

impl Captured<'_> {
  /// Samples buffered and ready to read.
  pub fn available(&self) -> usize {
    // SAFETY: the stream SDL is calling back for, alive for the callback.
    let bytes = unsafe { SDL_GetAudioStreamAvailable(self.stream) };
    bytes.max(0) as usize / SAMPLE_BYTES
  }

  /// Read samples into `dst` (non-blocking); the samples read, 0 on error.
  pub fn read(&mut self, dst: &mut [f32]) -> usize {
    // SAFETY: the stream SDL is calling back for; `dst` is valid for its
    // length in bytes.
    let bytes = unsafe {
      SDL_GetAudioStreamData(self.stream, dst.as_mut_ptr() as *mut c_void, (dst.len() * SAMPLE_BYTES) as c_int)
    };
    bytes.max(0) as usize / SAMPLE_BYTES
  }
}

/// SDL's put callback on a recording stream: runs on SDL's audio thread with
/// the stream's lock held, after the device put data in.
unsafe extern "C" fn on_captured_c(
  userdata: *mut c_void,
  stream: *mut SDL_AudioStream,
  _additional: c_int,
  _total: c_int,
) {
  // SAFETY: the userdata is the `Recording`'s boxed callback, alive until
  // the stream is destroyed; SDL calls one callback at a time per stream.
  let callback = unsafe { &mut *(userdata as *mut Capture) };
  let mut captured = Captured { stream, _callback: PhantomData };
  if std::panic::catch_unwind(AssertUnwindSafe(|| callback(&mut captured))).is_err() {
    log::error!("[ore::audio] The capture callback panicked");
  }
}
