use sdl3::sys::keyboard::{SDL_GetModState, SDL_HasKeyboard};
use sdl3::sys::mouse::SDL_HasMouse;
use sdl3::sys::power::{SDL_GetPowerInfo, SDL_PowerState};
use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::video::{SDL_GetSystemTheme, SDL_GetWindowDisplayScale, SDL_GetWindowSafeArea, SDL_SystemTheme};

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
  // Force the v4l2 camera backend on desktop Linux. Device removal is broken in
  // both SDL 3.4.8 Linux backends, but for different reasons: pipewire (the
  // Wayland default) never calls SDL_CameraDisconnected at all (empty
  // global_remove), while v4l2 only mis-gates it (its udev callback drops
  // removals because device_event reports class 0 on remove). The v4l2 bug is a
  // one-liner we filed upstream, so we sit on v4l2 to pick up the fix the moment
  // it ships; until then add works and removal is silently missed on both.
  // Normal priority, so an explicit SDL_CAMERA_DRIVER env var still overrides.
  // Must run before SDL_INIT_CAMERA. Not Android (own backend) or other OSes.
  #[cfg(target_os = "linux")]
  unsafe {
    use sdl3::sys::hints::{SDL_SetHint, SDL_HINT_CAMERA_DRIVER};
    SDL_SetHint(SDL_HINT_CAMERA_DRIVER, c"v4l2".as_ptr());
  }
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
  SDL_GetAudioRecordingDevices, SDL_GetAudioStreamAvailable, SDL_GetAudioStreamData, SDL_OpenAudioDeviceStream,
  SDL_ResumeAudioStreamDevice, SDL_AUDIO_DEVICE_DEFAULT_RECORDING, SDL_AUDIO_F32,
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

pub fn audio_stream_resume(stream: *mut SDL_AudioStream) -> bool {
  unsafe { SDL_ResumeAudioStreamDevice(stream) }
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

/// Publish SDL's JNI env + activity into the process-wide `ndk-context` so
/// JNI-using dependencies can reach the Android `JavaVM` and `Context`. iroh's
/// network monitoring (reached via `flux:p2p`) reads this; without it the first
/// `ndk_context::android_context()` call panics ("android context was not
/// initialized"). Call once, after `SDL_Init`, before any such dependency runs.
/// The activity reference and VM live for the whole process, so they are not
/// released.
#[cfg(target_os = "android")]
pub fn init_android_context() {
  use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};

  let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  if env_ptr.is_null() {
    log::error!("[alloy] SDL_GetAndroidJNIEnv returned null; ndk-context not initialized");
    return;
  }
  let env = match unsafe { jni::JNIEnv::from_raw(env_ptr) } {
    Ok(env) => env,
    Err(e) => {
      log::error!("[alloy] JNIEnv::from_raw failed: {e}");
      return;
    }
  };
  let vm = match env.get_java_vm() {
    Ok(vm) => vm,
    Err(e) => {
      log::error!("[alloy] could not get JavaVM for ndk-context: {e}");
      return;
    }
  };
  let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
  if activity_ptr.is_null() {
    log::error!("[alloy] SDL_GetAndroidActivity returned null; ndk-context not initialized");
    return;
  }
  // SDL returns a LOCAL ref, valid only on this thread/frame. iroh's network
  // monitoring touches the context from its own threads, so promote it to a
  // global ref and keep it for the process lifetime (forget -> no DeleteGlobalRef).
  let activity_obj = unsafe { jni::objects::JObject::from_raw(activity_ptr) };
  let global = match env.new_global_ref(&activity_obj) {
    Ok(g) => g,
    Err(e) => {
      log::error!("[alloy] new_global_ref(activity) failed: {e}");
      return;
    }
  };
  let activity_global = global.as_raw();
  std::mem::forget(global);
  unsafe {
    ndk_context::initialize_android_context(
      vm.get_java_vm_pointer() as *mut std::ffi::c_void,
      activity_global as *mut std::ffi::c_void,
    );
  }
}
