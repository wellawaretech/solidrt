use sdl3::sys::keyboard::{SDL_GetModState, SDL_HasKeyboard};
use sdl3::sys::mouse::SDL_HasMouse;
use sdl3::sys::power::{SDL_GetPowerInfo, SDL_PowerState};
use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::pixels::SDL_PixelFormat;
use sdl3::sys::surface::{SDL_CreateSurfaceFrom, SDL_DestroySurface};
use sdl3::sys::video::{
  SDL_GetSystemTheme, SDL_GetWindowDisplayScale, SDL_GetWindowSafeArea, SDL_SetWindowIcon, SDL_SystemTheme,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
  OnBattery,
  Charging,
  Charged,
  NoBattery,
  Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct PowerInfo {
  pub state: PowerState,
  pub percent: Option<u8>,
}

pub fn get_power_info() -> PowerInfo {
  let mut seconds: std::ffi::c_int = -1; //ignore, state and percentage only
  let mut percent: std::ffi::c_int = -1;
  let state = unsafe { SDL_GetPowerInfo(&mut seconds, &mut percent) };
  PowerInfo {
    state: match state {
      SDL_PowerState::ON_BATTERY => PowerState::OnBattery,
      SDL_PowerState::CHARGING => PowerState::Charging,
      SDL_PowerState::CHARGED => PowerState::Charged,
      SDL_PowerState::NO_BATTERY => PowerState::NoBattery,
      _ => PowerState::Unknown,
    },
    percent: if percent < 0 { None } else { Some(percent as u8) },
  }
}

// OS-level dark/light preference. Mirrored into a local enum: the sdl3 crate
// wraps SDL_GetSystemTheme, but its SystemTheme derives nothing (not even
// Clone), so it cannot ride in AlloyEvent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTheme {
  Dark,
  Light,
  Unknown,
}

pub fn system_theme() -> SystemTheme {
  match unsafe { SDL_GetSystemTheme() } {
    SDL_SystemTheme::DARK => SystemTheme::Dark,
    SDL_SystemTheme::LIGHT => SystemTheme::Light,
    _ => SystemTheme::Unknown,
  }
}

// Input device presence (connected, not necessarily in use); the sdl3 crate
// does not wrap SDL_HasKeyboard / SDL_HasMouse.
pub fn has_keyboard() -> bool {
  unsafe { SDL_HasKeyboard() }
}

pub fn has_mouse() -> bool {
  unsafe { SDL_HasMouse() }
}

// The crate wraps this on KeyboardUtil, but the input-devices event snapshot
// is built without an sdl handle in scope; the raw call takes no arguments.
pub fn has_screen_keyboard_support() -> bool {
  unsafe { sdl3::sys::keyboard::SDL_HasScreenKeyboardSupport() }
}

// Whether a physical keyboard is attached: SDL's device list where the
// backend maintains one, OR the platform-reported fact (Android, where SDL
// never registers keyboards).
pub fn physical_keyboard() -> bool {
  has_keyboard() || crate::hardware_keyboard()
}

// Session start with IME configuration. The crate wraps only the plain
// SDL_StartTextInput, so the typed variant goes through the raw properties
// API; unset options keep SDL's defaults (notably capitalization defaults to
// Sentences for plain text).
pub fn start_text_input_with_options(window: &sdl3::video::Window, opts: &crate::TextInputOptions) {
  use crate::{TextCapitalization, TextInputType};
  use sdl3::sys::keyboard::*;
  use sdl3::sys::properties::*;

  let input_type = opts.input_type.map(|t| match t {
    TextInputType::Text => SDL_TEXTINPUT_TYPE_TEXT,
    TextInputType::Name => SDL_TEXTINPUT_TYPE_TEXT_NAME,
    TextInputType::Email => SDL_TEXTINPUT_TYPE_TEXT_EMAIL,
    TextInputType::Username => SDL_TEXTINPUT_TYPE_TEXT_USERNAME,
    TextInputType::PasswordHidden => SDL_TEXTINPUT_TYPE_TEXT_PASSWORD_HIDDEN,
    TextInputType::PasswordVisible => SDL_TEXTINPUT_TYPE_TEXT_PASSWORD_VISIBLE,
    TextInputType::Number => SDL_TEXTINPUT_TYPE_NUMBER,
    TextInputType::NumberPasswordHidden => SDL_TEXTINPUT_TYPE_NUMBER_PASSWORD_HIDDEN,
    TextInputType::NumberPasswordVisible => SDL_TEXTINPUT_TYPE_NUMBER_PASSWORD_VISIBLE,
  });
  let capitalize = opts.capitalize.map(|c| match c {
    TextCapitalization::None => SDL_CAPITALIZE_NONE,
    TextCapitalization::Sentences => SDL_CAPITALIZE_SENTENCES,
    TextCapitalization::Words => SDL_CAPITALIZE_WORDS,
    TextCapitalization::Letters => SDL_CAPITALIZE_LETTERS,
  });

  unsafe {
    let props = SDL_CreateProperties();
    if let Some(t) = input_type {
      SDL_SetNumberProperty(props, SDL_PROP_TEXTINPUT_TYPE_NUMBER, t.0 as i64);
    }
    if let Some(c) = capitalize {
      SDL_SetNumberProperty(props, SDL_PROP_TEXTINPUT_CAPITALIZATION_NUMBER, c.0 as i64);
    }
    if let Some(a) = opts.autocorrect {
      SDL_SetBooleanProperty(props, SDL_PROP_TEXTINPUT_AUTOCORRECT_BOOLEAN, a);
    }
    if let Some(m) = opts.multiline {
      SDL_SetBooleanProperty(props, SDL_PROP_TEXTINPUT_MULTILINE_BOOLEAN, m);
    }
    SDL_StartTextInputWithProperties(window.raw(), props);
    SDL_DestroyProperties(props);
  }
}

// Window icon from straight-alpha RGBA8 pixels. The sdl3 crate does not wrap
// SDL_SetWindowIcon, so this goes through sdl3-sys directly. SDL copies the
// pixels into the surface's own representation on SetWindowIcon platforms and
// the surface only borrows `rgba`, so it is created, applied and destroyed
// within the call. Platforms without window icons (macOS) return Err.
pub fn set_window_icon(
  window: &sdl3::video::Window,
  width: u32,
  height: u32,
  rgba: &[u8],
) -> Result<(), String> {
  if rgba.len() != (width * height * 4) as usize {
    return Err(format!("icon pixel buffer is {} bytes, expected {}x{}x4", rgba.len(), width, height));
  }
  let surface = unsafe {
    SDL_CreateSurfaceFrom(
      width as i32,
      height as i32,
      SDL_PixelFormat::RGBA32,
      rgba.as_ptr() as *mut std::ffi::c_void,
      (width * 4) as i32,
    )
  };
  if surface.is_null() {
    return Err(sdl_error());
  }
  let ok = unsafe { SDL_SetWindowIcon(window.raw(), surface) };
  unsafe { SDL_DestroySurface(surface) };
  if ok {
    Ok(())
  } else {
    Err(sdl_error())
  }
}

pub fn window_safe_area(window: &sdl3::video::Window) -> SDL_Rect {
  let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
  unsafe { SDL_GetWindowSafeArea(window.raw(), &mut rect) };
  // SDL reports the safe area in SDL_GetWindowSize units (it builds the rect from
  // window->w/h minus the platform insets). Those units differ per platform:
  // desktop reports logical points there, but Android bakes the display density
  // into window->w/h (window->w == the physical surface width), so the safe-area
  // rect comes back in physical pixels. The layout works in logical points, which
  // we report elsewhere as size_in_pixels / display_scale, so normalize the rect
  // into that same space by the ratio (size_in_pixels / scale) / window_size.
  // That ratio is 1.0 on desktop (including fractional-scaled displays, where
  // window_size is already logical) and 1/scale on Android.
  let (lw, lh) = window.size();
  let (pw, ph) = window.size_in_pixels();
  let scale = window_display_scale(window);
  if lw > 0 && lh > 0 && scale > 0.0 {
    let fx = (pw as f32 / scale) / lw as f32;
    let fy = (ph as f32 / scale) / lh as f32;
    rect.x = (rect.x as f32 * fx).round() as i32;
    rect.w = (rect.w as f32 * fx).round() as i32;
    rect.y = (rect.y as f32 * fy).round() as i32;
    rect.h = (rect.h as f32 * fy).round() as i32;
  }
  rect
}

pub fn window_display_scale(window: &sdl3::video::Window) -> f32 {
  unsafe { SDL_GetWindowDisplayScale(window.raw()) }
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
use sdl3::sys::surface::{SDL_ConvertPixels, SDL_Surface};

pub fn camera_subsystem_init() -> bool {
  // No driver hint: SDL's own Linux order is v4l2 first, pipewire second, and
  // that is what we want. v4l2 is the proven backend (the MJPG format
  // workaround and hotplug-add were verified against it, and our one-line
  // device-removal fix targets it - written up in okf/upstream/, not filed
  // upstream yet - so the default preference picks it up if SDL takes the
  // fix). SDL's pipewire camera backend is not trustworthy today: it targets
  // nodes by node.description (target.object matches node.name/object.serial,
  // so the target never resolves), it ignores the stream ERROR state so a
  // failed start reports permission PENDING forever, and upstream has an open
  // never-acquires-a-frame issue (libsdl-org/SDL#11473) - all observed here
  // on desktop, 2026-08-01. It stays as SDL's fallback for v4l2-less systems,
  // nothing more.
  //
  // On a Raspberry Pi 4 (fresh Raspberry Pi OS, no camera attached) v4l2's
  // init never returns - it wedges probing the Pi's bcm2835 codec/isp/rpivid
  // /dev/videoN nodes - so THIS CALL CAN BLOCK FOREVER. That is why it runs
  // on the dedicated init worker (see camera::ensure_init), never on the UI
  // thread: a wedged init costs one parked thread and cameras stay absent,
  // not the window. Until upstream fixes a backend, the Pi simply has no
  // SDL-visible camera (CSI ribbon cameras are not plain V4L2 capture
  // devices anyway; only USB UVC ones would appear).
  //
  // SDL_CAMERA_DRIVER in the environment still selects a backend explicitly.
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

/// The latest frame, or null when no new frame is available. Must be returned
/// with `camera_release_frame` (do not free or hold across pumps).
pub fn camera_acquire_frame(camera: *mut SDL_Camera) -> *mut SDL_Surface {
  unsafe { SDL_AcquireCameraFrame(camera, std::ptr::null_mut()) }
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
  SDL_AudioDeviceID, SDL_AudioSpec, SDL_AudioStream, SDL_DestroyAudioStream, SDL_GetAudioDeviceName,
  SDL_GetAudioRecordingDevices, SDL_GetAudioStreamAvailable, SDL_GetAudioStreamData, SDL_GetAudioStreamQueued,
  SDL_OpenAudioDeviceStream, SDL_PauseAudioStreamDevice, SDL_PutAudioStreamData, SDL_ResumeAudioStreamDevice,
  SDL_SetAudioStreamGain, SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, SDL_AUDIO_DEVICE_DEFAULT_RECORDING, SDL_AUDIO_F32,
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
/// the device format). The stream starts paused; destroying it also closes
/// the device it opened.
pub fn audio_open_recording_stream(device: Option<u32>, sample_rate: u32) -> *mut SDL_AudioStream {
  let spec = SDL_AudioSpec { format: SDL_AUDIO_F32, channels: 1, freq: sample_rate as std::ffi::c_int };
  let devid = device.map(SDL_AudioDeviceID).unwrap_or(SDL_AUDIO_DEVICE_DEFAULT_RECORDING);
  unsafe { SDL_OpenAudioDeviceStream(devid, &spec, None, std::ptr::null_mut()) }
}

/// Open the default playback device bound to a new stream taking interleaved
/// f32 at `sample_rate`/`channels` on the app side (SDL converts to the
/// device format and mixes all bound streams natively). The stream starts
/// paused; destroying it also closes the device it opened.
pub fn audio_open_playback_stream(sample_rate: u32, channels: u16) -> *mut SDL_AudioStream {
  let spec =
    SDL_AudioSpec { format: SDL_AUDIO_F32, channels: channels as std::ffi::c_int, freq: sample_rate as std::ffi::c_int };
  unsafe { SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK, &spec, None, std::ptr::null_mut()) }
}

pub fn audio_stream_resume(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_ResumeAudioStreamDevice(stream) }
}

pub fn audio_stream_pause(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_PauseAudioStreamDevice(stream) }
}

/// Queue interleaved f32 samples for playback (non-blocking; SDL buffers).
pub fn audio_stream_put_f32(stream: *mut SDL_AudioStream, samples: &[f32]) -> bool {
  unsafe { SDL_PutAudioStreamData(stream, samples.as_ptr() as *const std::ffi::c_void, (samples.len() * 4) as std::ffi::c_int) }
}

/// Bytes queued on the stream's input side, not yet consumed by the device
/// (in the app-side spec's format).
pub fn audio_stream_queued_bytes(stream: *mut SDL_AudioStream) -> i32 {
  unsafe { SDL_GetAudioStreamQueued(stream) }
}

pub fn audio_stream_set_gain(stream: *mut SDL_AudioStream, gain: f32) -> bool {
  unsafe { SDL_SetAudioStreamGain(stream, gain) }
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

/// The swap interval for window presents: 1 everywhere. The blocking swap
/// is the frame pacer on desktop and the stock EGL sync path on Android;
/// async mode (0) was measured no better on the 2017 MediaTek TV during the
/// swap-latency investigation (okf/backlog/android-surface-swap-latency.md).
pub const WINDOW_SWAP_INTERVAL: i32 = 1;

/// Raise the calling thread to the platform's frame/display priority so
/// background processes cannot preempt it mid-frame; `critical` marks the
/// thread that owns the present deadline (raster), false the frame-building
/// tier (UI). This is what OS render threads themselves run at (Android HWUI
/// -4/-8, macOS user-interactive QoS, Windows above-normal). Measured on the
/// 2017 MediaTek TV: system services (cast_shell V8 GC) preempted default-
/// priority frame threads ~once a second, each a visible dropped frame; the
/// boost cut drops ~10x (okf/backlog/android-surface-swap-latency.md).
/// Best-effort everywhere: Android sanctions it, desktop Linux typically
/// denies negative nice to unprivileged processes (EPERM, logged at debug
/// and harmless), macOS and Windows always accept their calls.
pub fn frame_thread_priority(critical: bool) {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  unsafe {
    let nice = if critical { -8 } else { -4 };
    let tid = libc::gettid();
    if libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) != 0 {
      log::debug!("[alloy] setpriority({nice}) not permitted for thread {tid}");
    }
  }
  #[cfg(target_os = "macos")]
  unsafe {
    let class = if critical { libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE } else { libc::qos_class_t::QOS_CLASS_USER_INITIATED };
    if libc::pthread_set_qos_class_self_np(class, 0) != 0 {
      log::debug!("[alloy] pthread_set_qos_class_self_np failed");
    }
  }
  #[cfg(target_os = "windows")]
  unsafe {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_HIGHEST};
    let priority = if critical { THREAD_PRIORITY_HIGHEST } else { THREAD_PRIORITY_ABOVE_NORMAL };
    if SetThreadPriority(GetCurrentThread(), priority) == 0 {
      log::debug!("[alloy] SetThreadPriority({priority}) failed");
    }
  }
}

/// SDL's `Window::gl_swap_window` discards `SDL_GL_SwapWindow`'s result, but a
/// present can fail permanently (EGL context lost / D3D device removed under
/// ANGLE). Returns false on failure; the detail is in `sdl_error()`. Takes the
/// raw handle because the presenting UI thread has no `Window` (it lives on
/// the main thread); call only from the thread the GL context is current on.
pub fn gl_swap_window_checked(window: *mut sdl3::sys::video::SDL_Window) -> bool {
  unsafe { sdl3::sys::video::SDL_GL_SwapWindow(window) }
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

/// Android's own answer to "does this device have a touchscreen"
/// (PackageManager.hasSystemFeature). SDL's touch-device enumeration
/// over-reports on TV boxes whose virtual input drivers claim pointer
/// sources (measured: a Philips TPM171E lists a touch device while the
/// platform declares no android.hardware.touchscreen feature), so the
/// InputDevices touch fact gates on the platform feature too. Cached:
/// hardware features cannot change at runtime. Errors default to true so a
/// JNI hiccup can only ever fail toward the touch-device default, never
/// strip a real touchscreen of its policy.
#[cfg(target_os = "android")]
pub fn has_touchscreen_feature() -> bool {
  use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};
  use std::sync::OnceLock;
  static FEATURE: OnceLock<bool> = OnceLock::new();
  *FEATURE.get_or_init(|| {
    let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
    let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
    if env_ptr.is_null() || activity_ptr.is_null() {
      log::warn!("[alloy] no JNI env/activity for touchscreen feature query; assuming touch");
      return true;
    }
    let mut unowned = unsafe { jni::EnvUnowned::from_raw(env_ptr) };
    let outcome = unowned
      .with_env(|env| -> Result<bool, jni::errors::Error> {
        let activity = unsafe { jni::objects::JObject::from_raw(env, activity_ptr) };
        let pm = env
          .call_method(
            &activity,
            jni::jni_str!("getPackageManager"),
            jni::jni_sig!("()Landroid/content/pm/PackageManager;"),
            &[],
          )?
          .l()?;
        let feature = env.new_string("android.hardware.touchscreen")?;
        env
          .call_method(
            &pm,
            jni::jni_str!("hasSystemFeature"),
            jni::jni_sig!("(Ljava/lang/String;)Z"),
            &[jni::objects::JValue::Object(&feature)],
          )?
          .z()
      })
      .into_outcome();
    match outcome {
      jni::Outcome::Ok(has) => has,
      jni::Outcome::Err(e) => {
        log::warn!("[alloy] touchscreen feature query failed ({e}); assuming touch");
        true
      }
      jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
    }
  })
}

/// Publish SDL's JNI env + activity into the process-wide `ndk-context` so
/// JNI-using dependencies can reach the Android `JavaVM` and `Context`. iroh's
/// network monitoring (reached via `flux:p2p`) reads this; without it the first
/// `ndk_context::android_context()` call panics ("android context was not
/// initialized"). Call once, after `SDL_Init`, before any such dependency runs.
/// The activity reference and VM live for the whole process, so they are not
// --- SDL_mixer raw-pointer setters (audio ramp driver) ---
//
// The sdl3 crate wraps these on Track/Mixer objects, which are !Send, so the
// ramp thread (alloy::audio) cannot use them. SDL documents every one of
// these as safe to call from any thread; what the wrappers do NOT guarantee
// is pointer liveness - callers pass the raw MIX_Track/MIX_Mixer as usize and
// must keep it alive for the call (see the purge protocol on RampState).

/// MIX_SetTrackGain on a raw track pointer. Returns false on SDL error.
pub fn mix_track_set_gain_raw(track: usize, gain: f32) -> bool {
  unsafe { sdl3::mixer::sys::MIX_SetTrackGain(track as *mut sdl3::mixer::sys::MIX_Track, gain) }
}

/// MIX_SetTrackStereo (forced-stereo pan gains) on a raw track pointer.
pub fn mix_track_set_stereo_raw(track: usize, left: f32, right: f32) -> bool {
  let gains = sdl3::mixer::sys::MIX_StereoGains { left, right };
  unsafe { sdl3::mixer::sys::MIX_SetTrackStereo(track as *mut sdl3::mixer::sys::MIX_Track, &gains) }
}

/// MIX_SetTrackFrequencyRatio (playback rate) on a raw track pointer.
pub fn mix_track_set_frequency_ratio_raw(track: usize, ratio: f32) -> bool {
  unsafe { sdl3::mixer::sys::MIX_SetTrackFrequencyRatio(track as *mut sdl3::mixer::sys::MIX_Track, ratio) }
}

/// MIX_SetMixerGain (master) on a raw mixer pointer.
pub fn mix_mixer_set_gain_raw(mixer: usize, gain: f32) -> bool {
  unsafe { sdl3::mixer::sys::MIX_SetMixerGain(mixer as *mut sdl3::mixer::sys::MIX_Mixer, gain) }
}

/// released.
#[cfg(target_os = "android")]
pub fn init_android_context() {
  use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};

  let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  if env_ptr.is_null() {
    log::error!("[alloy] SDL_GetAndroidJNIEnv returned null; ndk-context not initialized");
    return;
  }
  let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
  if activity_ptr.is_null() {
    log::error!("[alloy] SDL_GetAndroidActivity returned null; ndk-context not initialized");
    return;
  }
  let mut unowned = unsafe { jni::EnvUnowned::from_raw(env_ptr) };
  let outcome = unowned
    .with_env(|env| -> Result<(), jni::errors::Error> {
      let vm = env.get_java_vm()?;
      // SDL returns a LOCAL ref, valid only on this thread/frame. iroh's network
      // monitoring touches the context from its own threads, so promote it to a
      // global ref and keep it for the process lifetime (into_raw -> no
      // DeleteGlobalRef).
      let activity_obj = unsafe { jni::objects::JObject::from_raw(env, activity_ptr) };
      let activity_global = env.new_global_ref(&activity_obj)?.into_raw();
      unsafe {
        ndk_context::initialize_android_context(
          vm.get_raw() as *mut std::ffi::c_void,
          activity_global as *mut std::ffi::c_void,
        );
      }
      Ok(())
    })
    .into_outcome();
  match outcome {
    jni::Outcome::Ok(()) => {}
    jni::Outcome::Err(e) => log::error!("[alloy] could not init ndk-context: {e}"),
    jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
  }
}
