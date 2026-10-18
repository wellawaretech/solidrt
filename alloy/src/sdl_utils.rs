use sdl3::sys::keyboard::{SDL_GetKeyFromScancode, SDL_GetModState, SDL_HasKeyboard};
use sdl3::sys::keycode::{SDL_Keymod, SDLK_UNKNOWN};
use sdl3::sys::mouse::{
  SDL_CreateAnimatedCursor, SDL_CreateColorCursor, SDL_CreateSystemCursor, SDL_Cursor, SDL_CursorFrameInfo,
  SDL_DestroyCursor, SDL_HasMouse, SDL_HideCursor, SDL_SetCursor, SDL_ShowCursor, SDL_SystemCursor,
};
use sdl3::sys::pixels::SDL_PixelFormat;
use sdl3::sys::power::{SDL_GetPowerInfo, SDL_PowerState};
use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::scancode::SDL_Scancode;
use sdl3::sys::surface::{SDL_AddSurfaceAlternateImage, SDL_CreateSurfaceFrom, SDL_DestroySurface, SDL_Surface};
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
pub fn set_window_icon(window: &sdl3::video::Window, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
  let surface = rgba_surface(width, height, rgba)?;
  let ok = unsafe { SDL_SetWindowIcon(window.raw(), surface) };
  unsafe { SDL_DestroySurface(surface) };
  if ok {
    Ok(())
  } else {
    Err(sdl_error())
  }
}

// A surface over straight-alpha RGBA8 pixels. No copy: `rgba` must outlive
// the surface, which the caller destroys.
fn rgba_surface(width: u32, height: u32, rgba: &[u8]) -> Result<*mut SDL_Surface, String> {
  if rgba.len() != (width as usize) * (height as usize) * 4 {
    return Err(format!("pixel buffer is {} bytes, expected {}x{}x4", rgba.len(), width, height));
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
    Err(sdl_error())
  } else {
    Ok(surface)
  }
}

/// An SDL cursor this process created, destroyed with it. SDL keeps showing
/// a set cursor until another is set, and destroying the current one reverts
/// to the default, so the owner outlives every use (the loop's cursor cache,
/// see crate::cursor).
pub struct OwnedCursor(*mut SDL_Cursor);

impl Drop for OwnedCursor {
  fn drop(&mut self) {
    unsafe { SDL_DestroyCursor(self.0) };
  }
}

fn owned_cursor(raw: *mut SDL_Cursor) -> Result<OwnedCursor, String> {
  if raw.is_null() {
    Err(sdl_error())
  } else {
    Ok(OwnedCursor(raw))
  }
}

pub fn create_system_cursor(shape: SDL_SystemCursor) -> Result<OwnedCursor, String> {
  owned_cursor(unsafe { SDL_CreateSystemCursor(shape) })
}

/// A cursor from straight-alpha RGBA8 frames (crate::CursorFrame): one frame
/// is a color cursor, more an animated one. SDL copies every image during
/// the call; the platform picks a frame's HiDPI alternate by size ratio.
pub fn create_image_cursor(frames: &[crate::CursorFrame], hot_x: u32, hot_y: u32) -> Result<OwnedCursor, String> {
  if frames.is_empty() {
    return Err("cursor has no frames".to_string());
  }
  let mut surfaces: Vec<*mut SDL_Surface> = Vec::with_capacity(frames.len());
  let mut built = Ok(());
  for frame in frames {
    match frame_surface(frame) {
      Ok(surface) => surfaces.push(surface),
      Err(e) => {
        built = Err(e);
        break;
      }
    }
  }
  let cursor = built.and_then(|()| {
    let raw = if surfaces.len() == 1 {
      unsafe { SDL_CreateColorCursor(surfaces[0], hot_x as i32, hot_y as i32) }
    } else {
      let mut infos: Vec<SDL_CursorFrameInfo> = surfaces
        .iter()
        .zip(frames)
        .map(|(&surface, frame)| SDL_CursorFrameInfo { surface, duration: frame.duration_ms })
        .collect();
      unsafe { SDL_CreateAnimatedCursor(infos.as_mut_ptr(), infos.len() as i32, hot_x as i32, hot_y as i32) }
    };
    owned_cursor(raw)
  });
  for surface in surfaces {
    unsafe { SDL_DestroySurface(surface) };
  }
  cursor
}

// The frame's 1x surface with its alternates attached. An attachment holds
// its own reference, so each alternate is released here; the pixels behind
// every surface stay borrowed from the frame until the base is destroyed.
fn frame_surface(frame: &crate::CursorFrame) -> Result<*mut SDL_Surface, String> {
  let (first, alternates) = frame.images.split_first().ok_or_else(|| "cursor frame has no image".to_string())?;
  let base = rgba_surface(first.width, first.height, &first.rgba)?;
  for image in alternates {
    let attached = rgba_surface(image.width, image.height, &image.rgba).and_then(|alternate| {
      let ok = unsafe { SDL_AddSurfaceAlternateImage(base, alternate) };
      unsafe { SDL_DestroySurface(alternate) };
      if ok {
        Ok(())
      } else {
        Err(sdl_error())
      }
    });
    if let Err(e) = attached {
      unsafe { SDL_DestroySurface(base) };
      return Err(e);
    }
  }
  Ok(base)
}

/// Show `cursor` from now on (SDL keeps the pointer until another is set).
pub fn set_cursor(cursor: &OwnedCursor) -> bool {
  unsafe { SDL_SetCursor(cursor.0) }
}

pub fn show_cursor(visible: bool) {
  unsafe {
    if visible {
      SDL_ShowCursor();
    } else {
      SDL_HideCursor();
    }
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

/// The instant an SDL event timestamp stands for. SDL stamps events in
/// nanoseconds on its own tick base (the platform's event time where the
/// backend has one, the receipt in the pump otherwise, never later than
/// now); `epoch` is that base as an Instant (see `ticks_epoch`).
pub fn event_instant(epoch: std::time::Instant, timestamp_ns: u64) -> std::time::Instant {
  // Windows is the exception: its event time is the message tick, which
  // moves in steps of the system timer, coarser than the receipt. Input is
  // not batched on a desktop and the pump blocks on the queue, so the
  // receipt is the better reading there.
  if cfg!(target_os = "windows") {
    return std::time::Instant::now();
  }
  (epoch + std::time::Duration::from_nanos(timestamp_ns)).min(std::time::Instant::now())
}

/// The next raw event off SDL's queue, or None when it is empty. The pump
/// polls raw beside the sdl3 crate's EventPump (main thread, one pump)
/// because the crate has no variant for every event it can hold: what it
/// does know converts with `sdl3::event::Event::from_ll`, the rest is read
/// here (see `finger_cancel`).
pub fn poll_raw_event() -> Option<sdl3::sys::events::SDL_Event> {
  let mut raw = std::mem::MaybeUninit::<sdl3::sys::events::SDL_Event>::uninit();
  // SDL fills the event exactly when it returns true.
  let filled = unsafe { sdl3::sys::events::SDL_PollEvent(raw.as_mut_ptr()) };
  filled.then(|| unsafe { raw.assume_init() })
}

/// `poll_raw_event` that waits up to `timeout_ms` for one.
pub fn wait_raw_event_timeout(timeout_ms: u32) -> Option<sdl3::sys::events::SDL_Event> {
  let mut raw = std::mem::MaybeUninit::<sdl3::sys::events::SDL_Event>::uninit();
  let filled = unsafe { sdl3::sys::events::SDL_WaitEventTimeout(raw.as_mut_ptr(), timeout_ms as i32) };
  filled.then(|| unsafe { raw.assume_init() })
}

/// A finger the system cancelled: SDL_EVENT_FINGER_CANCELED's payload.
/// `x`/`y` are normalized to the window, as SDL's finger events are.
pub struct FingerCancel {
  pub timestamp_ns: u64,
  pub finger_id: u64,
  pub x: f32,
  pub y: f32,
}

/// The raw event's finger cancel, or None for any other event. The sdl3
/// crate maps this event to `Event::Unknown` with the payload dropped
/// (okf/upstream/sdl3-no-finger-cancelled-variant.md), so it is read here.
pub fn finger_cancel(raw: &sdl3::sys::events::SDL_Event) -> Option<FingerCancel> {
  // Every member of the union starts with the type field, so it reads
  // validly for any event; the finger payload only once the type says so.
  let type_ = unsafe { raw.r#type };
  if type_ != sdl3::sys::events::SDL_EVENT_FINGER_CANCELED.0 {
    return None;
  }
  let finger = unsafe { raw.tfinger };
  Some(FingerCancel { timestamp_ns: finger.timestamp, finger_id: finger.fingerID.into(), x: finger.x, y: finger.y })
}

/// Now on SDL's tick timeline, ns since `ticks_epoch`: what SDL stamps
/// camera frames with, and what a session's open is stamped with.
pub fn ticks_ns() -> u64 {
  unsafe { sdl3::sys::timer::SDL_GetTicksNS() }
}

/// SDL's tick base as an Instant: the instant SDL_GetTicksNS counts from.
pub fn ticks_epoch() -> std::time::Instant {
  let now = std::time::Instant::now();
  let ticks = unsafe { sdl3::sys::timer::SDL_GetTicksNS() };
  now.checked_sub(std::time::Duration::from_nanos(ticks)).unwrap_or(now)
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
  // unbounded (libsdl-org/SDL#15085, open, milestoned 3.6.0; the same loop OOMs
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

/// Ask Android for a runtime permission (`android.permission.RECORD_AUDIO`
/// and the like) without waiting for the answer: `callback` runs once, on
/// the activity's thread, with `granted`, at once when the permission is
/// already held. SDL's own audio recording open asks for RECORD_AUDIO with
/// a blocking callback (SDL_aaudio.c spins until the answer), so a caller
/// asks here first and opens once granted, when SDL's own request answers
/// at once. False when the request could not be made.
#[cfg(target_os = "android")]
pub fn request_android_permission(
  permission: &std::ffi::CStr,
  callback: sdl3::sys::system::SDL_RequestAndroidPermissionCallback,
  userdata: *mut std::ffi::c_void,
) -> bool {
  unsafe { sdl3::sys::system::SDL_RequestAndroidPermission(permission.as_ptr(), callback, userdata) }
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
  thread_nice(if critical { -8 } else { -4 });
  #[cfg(target_os = "macos")]
  unsafe {
    let class = if critical {
      libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE
    } else {
      libc::qos_class_t::QOS_CLASS_USER_INITIATED
    };
    if libc::pthread_set_qos_class_self_np(class, 0) != 0 {
      log::debug!("[alloy] pthread_set_qos_class_self_np failed");
    }
  }
  #[cfg(target_os = "windows")]
  unsafe {
    use windows_sys::Win32::System::Threading::{
      GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_HIGHEST,
    };
    let priority = if critical { THREAD_PRIORITY_HIGHEST } else { THREAD_PRIORITY_ABOVE_NORMAL };
    if SetThreadPriority(GetCurrentThread(), priority) == 0 {
      log::debug!("[alloy] SetThreadPriority({priority}) failed");
    }
  }
}

/// The nice value of a latency worker on Android and Linux: between the UI
/// thread's -4 and the default 0, so the frame tier keeps its precedence
/// over the worker and the worker has its own over the process's
/// background threads. A thread inherits its spawner's nice, so for a
/// worker the UI thread starts this is a step down, not a boost. SDL's
/// own `SDL_THREAD_PRIORITY_HIGH` is -10 there, above the raster thread,
/// which is why the worker does not use it.
const LATENCY_WORKER_NICE: i32 = -2;

/// The utilization floor a latency worker asks the scheduler for, on the
/// kernel's 0..1024 capacity scale; 1024 is the largest core at its top
/// clock. A worker that runs tens of milliseconds a few times a second has
/// a low average utilization, and an energy-aware scheduler answers that
/// average with a little core at a low clock; the floor replaces the
/// average with what the burst needs.
const LATENCY_WORKER_UTIL_MIN: u32 = 1024;

/// The placement `latency_worker_thread_priority` gave a thread, kept so
/// the thread can re-assert it: the nice value and the utilization floor
/// stick, but a hard affinity does not survive a cpuset move (Android
/// rewrites a thread's allowed cores when the app changes between its
/// top-app, foreground and background groups, and the Pixel 7 returns from
/// the background with the pin gone), so a pinned worker calls
/// `reassert` before each job. While the app sits in a group without the
/// pinned cores the re-pin fails and the thread runs where the group
/// allows; the next job in the top-app group restores it.
pub struct LatencyPlacement {
  /// The cores the thread was pinned to; empty when it was not.
  cores: Vec<usize>,
}

impl LatencyPlacement {
  /// Re-apply the pin, when there is one.
  pub fn reassert(&self) {
    if !self.cores.is_empty() {
      pin_thread_to(&self.cores);
    }
  }
}

/// Give the calling thread the placement of a latency worker: a thread whose
/// duration is lag the user sees but never a frame's deadline (the sort
/// thread: the splat order lags the camera by one sort). Below the frame
/// tier (`frame_thread_priority`) and above the default, on a performance
/// core: on Linux a nice value plus a utilization floor through
/// `sched_setattr`, and where the floor is refused a hard affinity to the
/// cores above the smallest capacity. Android refuses the floor by policy
/// (its kernel makes any clamp change depend on CAP_SYS_NICE, which no app
/// has), so there the pin is the placement; measured on the Pixel 7 it is
/// what moves the sort, nice alone moves nothing. macOS places by QoS and a
/// thread at the default QoS already runs on the performance cores; Windows
/// has no placement hint; both are no-ops. Best-effort everywhere, like
/// `frame_thread_priority`: desktop Linux denies the nice, and the floor
/// only costs clock on a uniform CPU.
pub fn latency_worker_thread_priority() -> LatencyPlacement {
  thread_nice(LATENCY_WORKER_NICE);
  if thread_util_min(LATENCY_WORKER_UTIL_MIN) {
    return LatencyPlacement { cores: Vec::new() };
  }
  let cores = performance_cores();
  if !cores.is_empty() && pin_thread_to(&cores) {
    log::debug!("[alloy] latency worker pinned to cpus {cores:?}");
    LatencyPlacement { cores }
  } else {
    LatencyPlacement { cores: Vec::new() }
  }
}

/// Set the calling thread's nice value (Android and Linux; a no-op
/// elsewhere). Whether the kernel accepted it: desktop Linux typically
/// denies a negative value to an unprivileged process (EPERM, logged at
/// debug and harmless).
pub fn thread_nice(nice: i32) -> bool {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  unsafe {
    let tid = libc::gettid();
    if libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) != 0 {
      log::debug!("[alloy] setpriority({nice}) not permitted for thread {tid}");
      return false;
    }
    true
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    let _ = nice;
    false
  }
}

/// Ask the scheduler for a utilization floor for the calling thread
/// (Linux 5.3 and later with utilization clamping): `util` on the 0..1024
/// capacity scale, the policy and the nice value left as they are. Whether
/// the kernel took it: a kernel without the clamp refuses (ENOSYS,
/// EOPNOTSUPP or EINVAL) and Android refuses an unprivileged caller
/// (EPERM), both logged at debug.
pub fn thread_util_min(util: u32) -> bool {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  unsafe {
    // `struct sched_attr` with the clamp fields (SCHED_ATTR_SIZE_VER1); the
    // libc crate declares the older, shorter version and no wrapper.
    #[repr(C)]
    struct SchedAttr {
      size: u32,
      sched_policy: u32,
      sched_flags: u64,
      sched_nice: i32,
      sched_priority: u32,
      sched_runtime: u64,
      sched_deadline: u64,
      sched_period: u64,
      sched_util_min: u32,
      sched_util_max: u32,
    }
    const SCHED_ATTR_SIZE_VER1: u32 = 56;
    const SCHED_FLAG_KEEP_POLICY: u64 = 0x08;
    const SCHED_FLAG_KEEP_PARAMS: u64 = 0x10;
    const SCHED_FLAG_UTIL_CLAMP_MIN: u64 = 0x20;
    let attr = SchedAttr {
      size: SCHED_ATTR_SIZE_VER1,
      sched_policy: 0,
      sched_flags: SCHED_FLAG_KEEP_POLICY | SCHED_FLAG_KEEP_PARAMS | SCHED_FLAG_UTIL_CLAMP_MIN,
      sched_nice: 0,
      sched_priority: 0,
      sched_runtime: 0,
      sched_deadline: 0,
      sched_period: 0,
      sched_util_min: util,
      sched_util_max: 0,
    };
    let calling_thread: libc::pid_t = 0;
    let flags: libc::c_uint = 0;
    if libc::syscall(libc::SYS_sched_setattr, calling_thread, &attr as *const SchedAttr, flags) != 0 {
      log::debug!("[alloy] sched_setattr(util_min={util}) refused: {}", std::io::Error::last_os_error());
      return false;
    }
    true
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    let _ = util;
    false
  }
}

/// The cores above the smallest capacity, from the kernel's per-cpu
/// `cpu_capacity` (Android and Linux): on the Pixel 7 the two mid and two
/// big cores beside four little ones. Empty on a uniform CPU, where the
/// files say one capacity, or without the files.
pub fn performance_cores() -> Vec<usize> {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  {
    let max_cpus = std::mem::size_of::<libc::cpu_set_t>() * 8;
    let mut capacities: Vec<(usize, u32)> = Vec::new();
    for cpu in 0..max_cpus {
      let Ok(text) = std::fs::read_to_string(format!("/sys/devices/system/cpu/cpu{cpu}/cpu_capacity")) else { break };
      let Ok(capacity) = text.trim().parse::<u32>() else { break };
      capacities.push((cpu, capacity));
    }
    let Some(least) = capacities.iter().map(|&(_, capacity)| capacity).min() else { return Vec::new() };
    capacities.iter().filter(|&&(_, capacity)| capacity > least).map(|&(cpu, _)| cpu).collect()
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    Vec::new()
  }
}

/// Pin the calling thread to `cores` (Android and Linux; false elsewhere).
/// Whether the kernel took it: it refuses a set with no core the thread's
/// cpuset allows (EINVAL, logged at debug).
pub fn pin_thread_to(cores: &[usize]) -> bool {
  #[cfg(any(target_os = "android", target_os = "linux"))]
  unsafe {
    let mut set: libc::cpu_set_t = std::mem::zeroed();
    libc::CPU_ZERO(&mut set);
    for &cpu in cores {
      libc::CPU_SET(cpu, &mut set);
    }
    if libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
      log::debug!("[alloy] sched_setaffinity({cores:?}) refused: {}", std::io::Error::last_os_error());
      return false;
    }
    true
  }
  #[cfg(not(any(target_os = "android", target_os = "linux")))]
  {
    let _ = cores;
    false
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
/// Run `f` against SDL's JNI env and the activity object on the current
/// thread (SDL attaches the thread to the JVM). A JNI failure or a Java
/// exception comes back as the error, prefixed with `what`.
#[cfg(target_os = "android")]
pub(crate) fn with_android_activity<T>(
  what: &str,
  f: impl FnOnce(&mut jni::Env<'_>, &jni::objects::JObject<'_>) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
  use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};
  let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
  if env_ptr.is_null() || activity_ptr.is_null() {
    return Err(format!("no JNI env/activity for the {what}"));
  }
  let mut unowned = unsafe { jni::EnvUnowned::from_raw(env_ptr) };
  let outcome = unowned
    .with_env(|env| {
      let activity = unsafe { jni::objects::JObject::from_raw(env, activity_ptr) };
      f(env, &activity)
    })
    .into_outcome();
  match outcome {
    jni::Outcome::Ok(value) => Ok(value),
    jni::Outcome::Err(e) => Err(format!("{what} JNI call failed: {e}")),
    jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
  }
}

/// The strings of a Java `String[]`, in order.
#[cfg(target_os = "android")]
pub(crate) fn java_strings(
  env: &mut jni::Env<'_>,
  array: &jni::objects::JObjectArray<'_, jni::objects::JString<'_>>,
) -> Result<Vec<String>, jni::errors::Error> {
  let len = array.len(env)?;
  let mut out = Vec::with_capacity(len);
  for i in 0..len {
    let element = array.get_element(env, i)?;
    out.push(element.mutf8_chars(env)?.to_str().into_owned());
  }
  Ok(out)
}

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

/// Locate a stored (uncompressed) asset inside this app's own APK: the APK's
/// absolute path (`ApplicationInfo.sourceDir`) plus the asset's byte offset
/// and length within it. The production runtime uses this to read the packed
/// `.solapp` payload in place (ranged reads against the APK, no extraction;
/// see forge `fs::AssetsBase::Packed`). The file descriptor Android opens to
/// report the offset is closed again here; readers reopen the APK by path.
/// None (with a warning) when the asset is absent or compressed - the caller
/// treats that as "no payload".
#[cfg(target_os = "android")]
pub fn packed_asset_location(name: &str) -> Option<(std::path::PathBuf, u64, u64)> {
  use sdl3::sys::system::{SDL_GetAndroidActivity, SDL_GetAndroidJNIEnv};
  let env_ptr = unsafe { SDL_GetAndroidJNIEnv() } as *mut jni::sys::JNIEnv;
  let activity_ptr = unsafe { SDL_GetAndroidActivity() } as jni::sys::jobject;
  if env_ptr.is_null() || activity_ptr.is_null() {
    log::warn!("[alloy] no JNI env/activity for packed asset lookup");
    return None;
  }
  let asset_name = std::ffi::CString::new(name).ok()?;
  let mut unowned = unsafe { jni::EnvUnowned::from_raw(env_ptr) };
  let outcome = unowned
    .with_env(|env| -> Result<Option<(std::path::PathBuf, u64, u64)>, jni::errors::Error> {
      let activity = unsafe { jni::objects::JObject::from_raw(env, activity_ptr) };
      let info = env
        .call_method(
          &activity,
          jni::jni_str!("getApplicationInfo"),
          jni::jni_sig!("()Landroid/content/pm/ApplicationInfo;"),
          &[],
        )?
        .l()?;
      let source_dir = env.get_field(&info, jni::jni_str!("sourceDir"), jni::jni_sig!("Ljava/lang/String;"))?.l()?;
      let apk = unsafe { jni::objects::JString::from_raw(env, source_dir.as_raw()) }.try_to_string(env)?;
      let assets = env
        .call_method(&activity, jni::jni_str!("getAssets"), jni::jni_sig!("()Landroid/content/res/AssetManager;"), &[])?
        .l()?;
      // The AAssetManager is owned by the Java AssetManager (held alive by
      // the activity for the process lifetime); no ownership transfers here.
      // The casts bridge the jni crate (jni-sys 0.4) and ndk-sys (jni-sys
      // 0.3), which name the same ABI types in different crates.
      let manager = unsafe { ndk_sys::AAssetManager_fromJava(env.get_raw() as *mut _, assets.as_raw() as *mut _) };
      let Some(manager) = std::ptr::NonNull::new(manager) else { return Ok(None) };
      let manager = unsafe { ndk::asset::AssetManager::from_ptr(manager) };
      let Some(asset) = manager.open(&asset_name) else { return Ok(None) };
      // Fails for a compressed asset, which has no contiguous bytes to point
      // at - the packer stores the payload uncompressed for this reason.
      let fd = match asset.open_file_descriptor() {
        Ok(fd) => fd,
        Err(e) => {
          log::warn!("[alloy] asset {name} has no readable file range (stored uncompressed?): {e}");
          return Ok(None);
        }
      };
      Ok(Some((std::path::PathBuf::from(apk), fd.offset as u64, fd.size as u64)))
    })
    .into_outcome();
  match outcome {
    jni::Outcome::Ok(location) => location,
    jni::Outcome::Err(e) => {
      log::warn!("[alloy] packed asset lookup failed: {e}");
      None
    }
    jni::Outcome::Panic(payload) => std::panic::resume_unwind(payload),
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
