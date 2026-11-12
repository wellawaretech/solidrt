use sdl3::sys::keyboard::{SDL_GetKeyFromScancode, SDL_GetModState};
use sdl3::sys::keycode::{SDL_Keymod, SDLK_UNKNOWN};
use sdl3::sys::scancode::SDL_Scancode;
use sdl3::sys::surface::SDL_Surface;

// The keycode a physical key produces under a modifier state, through the
// active layout, as SDL's raw codepoint. The crate's `Keycode::from_scancode`
// wraps the same call but maps the result through its enum of named
// constants, which has no entry for a shifted letter ("C" is 0x43, not a
// constant), so a Shift+letter lookup came back None through it. None when
// SDL knows no key for the scancode.
pub fn key_from_scancode(scancode: sdl3::keyboard::Scancode, modstate: SDL_Keymod) -> Option<u32> {
  let key = unsafe { SDL_GetKeyFromScancode(SDL_Scancode(scancode as i32), modstate, false) };
  (key != SDLK_UNKNOWN).then_some(key.0)
}

pub fn mod_state() -> sdl3::keyboard::Mod {
  unsafe { sdl3::keyboard::Mod::from_bits(SDL_GetModState().0).unwrap_or(sdl3::keyboard::Mod::NOMOD) }
}

// --- Camera (SDL camera subsystem; not exposed by the sdl3 crate) -----------
//
// Thin unsafe-call wrappers only; camera session logic lives in crate::camera.

use sdl3::sys::camera::{
  SDL_AcquireCameraFrame, SDL_Camera, SDL_CameraID, SDL_CameraPermissionState, SDL_CameraPosition, SDL_CameraSpec,
  SDL_CloseCamera, SDL_GetCameraFormat, SDL_GetCameraName, SDL_GetCameraPermissionState, SDL_GetCameraPosition,
  SDL_GetCameraSupportedFormats, SDL_GetCameras, SDL_OpenCamera, SDL_ReleaseCameraFrame,
};
use sdl3::sys::init::{SDL_InitSubSystem, SDL_INIT_CAMERA};
use sdl3::sys::pixels::SDL_PIXELFORMAT_RGBA32;
use sdl3::sys::stdinc::SDL_free;
use sdl3::sys::surface::SDL_ConvertPixels;

pub fn camera_subsystem_init() -> bool {
  // No driver hint by default: SDL's own Linux order is v4l2 first, pipewire
  // second, and that is what we want. v4l2 is the proven backend (the MJPG
  // format workaround and hotplug-add were verified against it, and our
  // one-line device-removal fix targets it - written up in okf/upstream/, not
  // filed upstream yet - so the default preference picks it up if SDL takes
  // the fix). SDL's pipewire camera backend is not trustworthy today: it
  // targets nodes by node.description (target.object matches
  // node.name/object.serial, so the target never resolves), it ignores the
  // stream ERROR state so a failed start reports permission PENDING forever,
  // and upstream has an open never-acquires-a-frame issue
  // (libsdl-org/SDL#11473) - all observed here on desktop, 2026-08-01. It
  // stays as SDL's fallback for v4l2-less systems, nothing more.
  //
  // THIS CALL CAN BURN TWO CORES FOREVER, so nothing may call it
  // speculatively - only an app actually opening a camera. v4l2's enumeration
  // does not terminate on hardware that exposes stepwise frame-size ranges
  // (Raspberry Pi 4, 2026-08-26): the bcm2835 codec/isp /dev/videoN nodes
  // advertise "32x32 - 16384x16384 with step 2/2" for ~20 formats each, and
  // SDL walks every size in that range - ~67 million VIDIOC_ENUM_FRAMEINTERVALS
  // calls per format, measured at ~38k/s, all failing. That is not slow, it is
  // unbounded (libsdl-org/SDL#15085, a duplicate of #16273, open, milestoned 3.6.0; the same loop OOMs
  // instead where the interval ioctl succeeds).
  //
  // Running it off the UI thread does not contain it. SDL_UDEV_Scan matches the
  // input, sound AND video4linux subsystems and hands every device to every
  // registered callback, so once the camera subsystem registers its callback
  // the MAIN thread's gamepad init (LINUX_JoystickInit -> SDL_UDEV_Scan ->
  // MaybeAddDevice -> AddCameraFormat) enters the same loop and never reaches
  // the event pump: no ticks, no frames, a black window, both threads at 100%.
  //
  // Hence the player shows its scan button without enumerating cameras
  // first (apps/player/src/parts/home-screen.tsx): a machine whose backend
  // cannot enumerate stays fully usable as long as nothing asks it to.
  unsafe { SDL_InitSubSystem(SDL_INIT_CAMERA) }
}

pub fn camera_ids() -> Vec<u32> {
  let mut count: std::ffi::c_int = 0;
  let ids = unsafe { SDL_GetCameras(&mut count) };
  if ids.is_null() {
    return Vec::new();
  }
  let result = (0..count as usize).map(|i| unsafe { (*ids.add(i)).0 }).collect();
  unsafe { SDL_free(ids as *mut std::ffi::c_void) };
  result
}

pub fn camera_name(id: u32) -> String {
  let name = unsafe { SDL_GetCameraName(SDL_CameraID(id)) };
  if name.is_null() {
    return String::new();
  }
  unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned()
}

pub fn camera_position(id: u32) -> SDL_CameraPosition {
  unsafe { SDL_GetCameraPosition(SDL_CameraID(id)) }
}

/// The native capture specs the camera offers (format/size/framerate combos).
pub fn camera_supported_formats(id: u32) -> Vec<SDL_CameraSpec> {
  let mut count: std::ffi::c_int = 0;
  let specs = unsafe { SDL_GetCameraSupportedFormats(SDL_CameraID(id), &mut count) };
  if specs.is_null() {
    return Vec::new();
  }
  let result = (0..count as usize).map(|i| unsafe { *(*specs.add(i)) }).collect();
  unsafe { SDL_free(specs as *mut std::ffi::c_void) };
  result
}

pub fn camera_open(id: u32, spec: &SDL_CameraSpec) -> *mut SDL_Camera {
  unsafe { SDL_OpenCamera(SDL_CameraID(id), spec) }
}

/// Convert a frame surface into tightly packed RGBA32; `dst` must hold
/// exactly `w * h * 4` bytes.
pub fn surface_to_rgba(surface: &SDL_Surface, dst: &mut [u8]) -> bool {
  debug_assert_eq!(dst.len(), (surface.w as usize) * (surface.h as usize) * 4);
  unsafe {
    SDL_ConvertPixels(
      surface.w,
      surface.h,
      surface.format,
      surface.pixels,
      surface.pitch,
      SDL_PIXELFORMAT_RGBA32,
      dst.as_mut_ptr() as *mut std::ffi::c_void,
      surface.w * 4,
    )
  }
}

pub fn camera_permission(camera: *mut SDL_Camera) -> SDL_CameraPermissionState {
  unsafe { SDL_GetCameraPermissionState(camera) }
}

/// The spec frames are delivered in (valid once permission is approved).
pub fn camera_format(camera: *mut SDL_Camera) -> Option<SDL_CameraSpec> {
  let mut spec = SDL_CameraSpec::default();
  if unsafe { SDL_GetCameraFormat(camera, &mut spec) } {
    Some(spec)
  } else {
    None
  }
}

/// The oldest queued frame and its capture timestamp (SDL tick timeline,
/// nanoseconds), or a null surface when no new frame is available. Must be
/// returned with `camera_release_frame` (do not free or hold across polls).
pub fn camera_acquire_frame(camera: *mut SDL_Camera) -> (*mut SDL_Surface, u64) {
  let mut timestamp_ns: u64 = 0;
  let surface = unsafe { SDL_AcquireCameraFrame(camera, &mut timestamp_ns) };
  (surface, timestamp_ns)
}

pub fn camera_release_frame(camera: *mut SDL_Camera, frame: *mut SDL_Surface) {
  unsafe { SDL_ReleaseCameraFrame(camera, frame) };
}

pub fn camera_close(camera: *mut SDL_Camera) {
  unsafe { SDL_CloseCamera(camera) };
}

// --- Audio recording (the sdl3 crate's safe audio API is unusable here:
// AudioSubsystem is !Send and main-thread-bound, while capture sessions live
// on the UI thread) --------------------------------------------------------
//
// Thin unsafe-call wrappers only; microphone session logic lives in
// crate::microphone.

use sdl3::sys::audio::{
  SDL_AudioDeviceID, SDL_AudioSpec, SDL_AudioStream, SDL_AudioStreamCallback, SDL_ClearAudioStream,
  SDL_CreateAudioStream, SDL_DestroyAudioStream, SDL_FlushAudioStream, SDL_GetAudioDeviceFormat,
  SDL_GetAudioDeviceName, SDL_GetAudioRecordingDevices, SDL_GetAudioStreamAvailable, SDL_GetAudioStreamData,
  SDL_GetAudioStreamQueued, SDL_OpenAudioDeviceStream, SDL_PauseAudioStreamDevice, SDL_PutAudioStreamData,
  SDL_ResumeAudioStreamDevice, SDL_SetAudioStreamFrequencyRatio, SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,
  SDL_AUDIO_DEVICE_DEFAULT_RECORDING, SDL_AUDIO_F32,
};
use sdl3::sys::init::SDL_INIT_AUDIO;

pub fn audio_subsystem_init() -> bool {
  unsafe { SDL_InitSubSystem(SDL_INIT_AUDIO) }
}

pub fn audio_recording_ids() -> Vec<u32> {
  let mut count: std::ffi::c_int = 0;
  let ids = unsafe { SDL_GetAudioRecordingDevices(&mut count) };
  if ids.is_null() {
    return Vec::new();
  }
  let result = (0..count as usize).map(|i| unsafe { (*ids.add(i)).0 }).collect();
  unsafe { SDL_free(ids as *mut std::ffi::c_void) };
  result
}

pub fn audio_device_name(id: u32) -> String {
  let name = unsafe { SDL_GetAudioDeviceName(SDL_AudioDeviceID(id)) };
  if name.is_null() {
    return String::new();
  }
  unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned()
}

/// Open a recording device (None = system default) bound to a new stream
/// delivering mono f32 at `sample_rate` on the app side (SDL converts from
/// the device format). `callback` is the stream's put callback: SDL calls it
/// on its audio thread, with the stream's lock held, each time the device
/// put data in, and the callback may read the stream right there. The
/// stream starts paused; destroying it also closes the device it opened.
pub fn audio_open_recording_stream(
  device: Option<u32>,
  sample_rate: u32,
  callback: SDL_AudioStreamCallback,
  userdata: *mut std::ffi::c_void,
) -> *mut SDL_AudioStream {
  let spec = SDL_AudioSpec { format: SDL_AUDIO_F32, channels: 1, freq: sample_rate as std::ffi::c_int };
  let devid = device.map(SDL_AudioDeviceID).unwrap_or(SDL_AUDIO_DEVICE_DEFAULT_RECORDING);
  unsafe { SDL_OpenAudioDeviceStream(devid, &spec, callback, userdata) }
}

/// The default playback device's own format: (sample rate, channels, the
/// device's buffer size in frames). None when the device cannot be queried.
pub fn audio_playback_device_format() -> Option<(u32, u16, u32)> {
  let mut spec = SDL_AudioSpec { format: SDL_AUDIO_F32, channels: 0, freq: 0 };
  let mut frames: std::ffi::c_int = 0;
  if !unsafe { SDL_GetAudioDeviceFormat(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &mut spec, &mut frames) } {
    return None;
  }
  Some((spec.freq.max(0) as u32, spec.channels.max(0) as u16, frames.max(0) as u32))
}

/// Open the default playback device bound to a new stream taking interleaved
/// f32 at `sample_rate`/`channels` on the app side (SDL converts to the
/// device format). `callback` is the stream's get callback: SDL calls it on
/// its audio thread, with the stream's lock held, each time the device
/// wants more than the stream holds, and the callback may put data right
/// there. The stream starts paused; destroying it also closes the device
/// it opened.
pub fn audio_open_playback_stream(
  sample_rate: u32,
  channels: u16,
  callback: SDL_AudioStreamCallback,
  userdata: *mut std::ffi::c_void,
) -> *mut SDL_AudioStream {
  let spec = SDL_AudioSpec {
    format: SDL_AUDIO_F32,
    channels: channels as std::ffi::c_int,
    freq: sample_rate as std::ffi::c_int,
  };
  unsafe { SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, callback, userdata) }
}

/// An unbound stream converting interleaved f32 from `src` to `dst` (each a
/// (sample rate, channels) pair): the resampler and channel converter a
/// voice renders through. Not tied to a device; destroy it when done.
pub fn audio_stream_create(src: (u32, u16), dst: (u32, u16)) -> *mut SDL_AudioStream {
  let spec = |(rate, channels): (u32, u16)| SDL_AudioSpec {
    format: SDL_AUDIO_F32,
    channels: channels as std::ffi::c_int,
    freq: rate as std::ffi::c_int,
  };
  unsafe { SDL_CreateAudioStream(&spec(src), &spec(dst)) }
}

/// No more input is coming: convert everything queued so it can be read.
pub fn audio_stream_flush(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_FlushAudioStream(stream) }
}

/// The rate input is consumed at relative to the stream's own conversion
/// (1.0 = as created; the range is 0.01 to 100).
pub fn audio_stream_set_frequency_ratio(stream: *mut SDL_AudioStream, ratio: f32) -> bool {
  unsafe { SDL_SetAudioStreamFrequencyRatio(stream, ratio) }
}

pub fn audio_stream_resume(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_ResumeAudioStreamDevice(stream) }
}

pub fn audio_stream_pause(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_PauseAudioStreamDevice(stream) }
}

/// Queue interleaved f32 samples for playback (non-blocking; SDL buffers).
pub fn audio_stream_put_f32(stream: *mut SDL_AudioStream, samples: &[f32]) -> bool {
  unsafe {
    SDL_PutAudioStreamData(stream, samples.as_ptr() as *const std::ffi::c_void, (samples.len() * 4) as std::ffi::c_int)
  }
}

/// Bytes queued on the stream's input side, not yet consumed by the device
/// (in the app-side spec's format).
pub fn audio_stream_queued_bytes(stream: *mut SDL_AudioStream) -> i32 {
  unsafe { SDL_GetAudioStreamQueued(stream) }
}

/// Drop everything queued on the stream (input and converted output).
pub fn audio_stream_clear(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_ClearAudioStream(stream) }
}

/// Bytes buffered in the stream, already converted to the app-side spec.
pub fn audio_stream_available(stream: *mut SDL_AudioStream) -> i32 {
  unsafe { SDL_GetAudioStreamAvailable(stream) }
}

/// Drain converted samples into `dst` (non-blocking); returns the number of
/// samples written, or -1 on error.
pub fn audio_stream_read_f32(stream: *mut SDL_AudioStream, dst: &mut [f32]) -> i32 {
  let bytes = unsafe {
    SDL_GetAudioStreamData(stream, dst.as_mut_ptr() as *mut std::ffi::c_void, (dst.len() * 4) as std::ffi::c_int)
  };
  if bytes < 0 {
    -1
  } else {
    bytes / 4
  }
}

pub fn audio_stream_destroy(stream: *mut SDL_AudioStream) {
  unsafe { SDL_DestroyAudioStream(stream) };
}

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

/// Degrees (clockwise, snapped to 0/90/180/270) to rotate an acquired camera
/// frame for an upright image; SDL sets this per frame on mobile (sensor
/// orientation + current display rotation), absent means 0.
pub fn surface_rotation_degrees(frame: *mut SDL_Surface) -> u32 {
  use sdl3::sys::properties::SDL_GetFloatProperty;
  use sdl3::sys::surface::{SDL_GetSurfaceProperties, SDL_PROP_SURFACE_ROTATION_FLOAT};

  let props = unsafe { SDL_GetSurfaceProperties(frame) };
  let degrees = unsafe { SDL_GetFloatProperty(props, SDL_PROP_SURFACE_ROTATION_FLOAT, 0.0) };
  let normalized = ((degrees.round() as i32 % 360) + 360) % 360;
  match normalized {
    90 | 180 | 270 => normalized as u32,
    0 => 0,
    other => {
      log::warn!("[camera] non-quadrant frame rotation {other}, treating as 0");
      0
    }
  }
}
