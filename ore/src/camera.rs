//! The camera facility: the subsystem's non-blocking start (`start`), the
//! device list (`devices`), an open camera (`Camera`) and the frames it
//! delivers (`Frame`). The session logic over it (permission polling,
//! deadlines, the producer thread, the textures) is alloy's.
//!
//! The device never signals: permission and frames are polled. The
//! hotplug events SDL delivers once the subsystem is up arrive through the
//! pump (ore's pump -> CameraDeviceChange), so a listener re-enumerates
//! without polling.

use std::sync::Mutex;
use std::time::Duration;

use sdl3::sys::camera::{
  SDL_AcquireCameraFrame, SDL_Camera, SDL_CameraID, SDL_CameraPermissionState, SDL_CameraPosition, SDL_CameraSpec,
  SDL_CloseCamera, SDL_GetCameraFormat, SDL_GetCameraName, SDL_GetCameraPermissionState, SDL_GetCameraPosition,
  SDL_GetCameraSupportedFormats, SDL_GetCameras, SDL_GetCurrentCameraDriver, SDL_OpenCamera, SDL_ReleaseCameraFrame,
};
use sdl3::sys::init::{SDL_InitSubSystem, SDL_INIT_CAMERA};
use sdl3::sys::pixels::{SDL_COLORSPACE_SRGB, SDL_PIXELFORMAT_MJPG, SDL_PIXELFORMAT_RGBA32};
use sdl3::sys::properties::SDL_GetFloatProperty;
use sdl3::sys::stdinc::SDL_free;
use sdl3::sys::surface::{SDL_ConvertPixels, SDL_GetSurfaceProperties, SDL_Surface, SDL_PROP_SURFACE_ROTATION_FLOAT};

/// The frame rate a spec choice prefers among equally sized native specs,
/// in thousandths of a frame per second: the usual camera rate, smooth
/// enough for a viewfinder without the cost of a high-rate mode.
const PREFERRED_FPS_MILLI: i64 = 30_000;

/// The frame rate asked for when the camera offers only compressed formats
/// and SDL converts to RGBA32 itself (see `Camera::open`).
const FALLBACK_FPS: i32 = 30;

/// Bytes per RGBA32 pixel.
const RGBA_BYTES: usize = 4;

/// What `start` reports. `Starting` is the normal first-call state.
pub enum Start {
  Ready,
  Starting,
  Failed(String),
}

enum InitState {
  NotStarted,
  Starting,
  Done(Result<(), String>),
}

static INIT_STATE: Mutex<InitState> = Mutex::new(InitState::NotStarted);

/// Lazy one-time SDL camera subsystem init (the first list or open). Never
/// blocks: the first call spawns a worker and reports `Starting`; a list
/// treats that as "no cameras yet", an open waits for it. When the worker
/// finishes, the devices it found arrive as SDL CAMERA_DEVICE_ADDED events
/// through the normal hotplug path (ore's pump -> CameraDeviceChange), so
/// listeners re-enumerate without polling.
///
/// The worker exists because SDL_INIT_CAMERA probes devices synchronously and
/// can wedge outright - on a Raspberry Pi 4 the v4l2 backend never returns
/// from the bcm2835 codec/isp/rpivid /dev/videoN nodes - and this used to run
/// on the UI thread inside the app's first render, where a wedge meant the
/// first frame was never built and (on Wayland) no window ever appeared.
///
/// The worker does NOT contain such a wedge, so do not treat it as the
/// answer to one (established on a Pi 4, 2026-08-26): SDL_UDEV_Scan's device
/// callbacks are process-global, so a camera backend stuck enumerating
/// formats also captures the main thread's gamepad init, which walks the
/// same scan. Both threads then spin at 100% CPU and the client never draws
/// a frame. A backend that can wedge must be kept from initializing at all
/// until an app actually wants a camera - which is why the player no longer
/// enumerates cameras to decide whether to show its scan button.
///
/// No other SDL_InitSubSystem call may race this one (SDL's init refcounting
/// is not thread-safe). Startup order is not the guarantee it looks like:
/// gamepad init is itself a udev scan and can still be running when the
/// first script calls in, which is exactly the overlap above.
pub fn start() -> Start {
  let mut state = INIT_STATE.lock().expect("camera init state poisoned");
  match &*state {
    InitState::NotStarted => {
      let spawned = std::thread::Builder::new().name("sol-camera-init".into()).spawn(|| {
        let result = if subsystem_init() {
          Ok(())
        } else {
          // SDL's error string is thread-local: capture it here, on the
          // thread the init actually ran on.
          Err(format!("camera subsystem init failed: {}", crate::sdl_error()))
        };
        if let Err(e) = &result {
          log::warn!("[ore::camera] {e}");
        }
        *INIT_STATE.lock().expect("camera init state poisoned") = InitState::Done(result);
      });
      match spawned {
        Ok(_) => {
          *state = InitState::Starting;
          Start::Starting
        }
        Err(e) => {
          let msg = format!("camera init thread failed to spawn: {e}");
          *state = InitState::Done(Err(msg.clone()));
          Start::Failed(msg)
        }
      }
    }
    InitState::Starting => Start::Starting,
    InitState::Done(Ok(())) => Start::Ready,
    InitState::Done(Err(e)) => Start::Failed(e.clone()),
  }
}

fn subsystem_init() -> bool {
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
  //
  // SAFETY: a plain subsystem init with no pointers; `start` runs it once,
  // on its worker, and no other SDL_InitSubSystem call may race it (see
  // `start`).
  unsafe { SDL_InitSubSystem(SDL_INIT_CAMERA) }
}

/// Whether opening a camera on this backend can show a consent prompt the
/// user answers, so a `Pending` permission may be a person deciding. False
/// means a long `Pending` is a wedged backend, and an embedder may fail the
/// open after a deadline.
///
/// Neither Linux backend ever shows a consent prompt. SDL's pipewire backend
/// reports permission only when the pw stream starts streaming and silently
/// swallows the stream ERROR state, so a failed negotiation stays PENDING
/// forever (observed with a working webcam, 2026-08-01). On Android, macOS
/// and Windows, Pending legitimately means the OS permission dialog is up.
///
/// Before the subsystem is up (`start` not yet `Ready`) no backend is known,
/// and the answer is true. The default leans toward "a prompt may be up": a
/// backend this function does not know gets no deadline rather than a wrong
/// one.
pub fn permission_prompts() -> bool {
  // SAFETY: SDL_GetCurrentCameraDriver takes no arguments and is thread
  // safe; it returns a static NUL-terminated string, or null when the camera
  // subsystem is not initialized, and the string is copied out at once.
  let name = unsafe {
    let raw = SDL_GetCurrentCameraDriver();
    (!raw.is_null()).then(|| std::ffi::CStr::from_ptr(raw).to_string_lossy().into_owned())
  };
  prompts_for_driver(name.as_deref())
}

/// The prompt fact for an SDL camera driver name (None: no driver yet).
pub(crate) fn prompts_for_driver(name: Option<&str>) -> bool {
  // SDL's Linux camera backends; neither has a consent prompt. Every other
  // name, known or new, may prompt.
  !matches!(name, Some("v4l2" | "pipewire"))
}

/// Which way a camera faces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
  Front,
  Back,
  Unknown,
}

/// One camera device.
pub struct CameraInfo {
  /// SDL's id; they start at 1.
  pub id: u32,
  pub name: String,
  pub facing: Facing,
}

/// The camera devices. Valid once `start` reports `Ready` (empty before);
/// the ids are SDL's (they start at 1).
pub fn devices() -> Vec<CameraInfo> {
  let mut count: std::ffi::c_int = 0;
  // SAFETY: `count` is a live out-parameter; the returned array is SDL's
  // allocation, null-checked here and freed below.
  let ids = unsafe { SDL_GetCameras(&mut count) };
  if ids.is_null() {
    return Vec::new();
  }
  let result = (0..count.max(0) as usize)
    .map(|i| {
      // SAFETY: SDL returned `count` ids at `ids`; `i` is below that.
      let id = unsafe { *ids.add(i) };
      CameraInfo { id: id.0, name: name(id), facing: facing(id) }
    })
    .collect();
  // SAFETY: `ids` is SDL's allocation from SDL_GetCameras, freed once.
  unsafe { SDL_free(ids as *mut std::ffi::c_void) };
  result
}

fn name(id: SDL_CameraID) -> String {
  // SAFETY: SDL_GetCameraName takes an id by value; the string it returns
  // is SDL's, null-checked and copied out below.
  let name = unsafe { SDL_GetCameraName(id) };
  if name.is_null() {
    return String::new();
  }
  // SAFETY: a non-null name is a NUL-terminated string SDL keeps alive.
  unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned()
}

fn facing(id: SDL_CameraID) -> Facing {
  // SAFETY: SDL_GetCameraPosition takes an id by value and returns a plain
  // enum (UNKNOWN for an id it does not know).
  match unsafe { SDL_GetCameraPosition(id) } {
    SDL_CameraPosition::FRONT_FACING => Facing::Front,
    SDL_CameraPosition::BACK_FACING => Facing::Back,
    _ => Facing::Unknown,
  }
}

/// The native capture specs the camera offers (format, size, frame rate).
fn supported_formats(id: u32) -> Vec<SDL_CameraSpec> {
  let mut count: std::ffi::c_int = 0;
  // SAFETY: `count` is a live out-parameter; the returned array is SDL's
  // allocation, null-checked here and freed below.
  let specs = unsafe { SDL_GetCameraSupportedFormats(SDL_CameraID(id), &mut count) };
  if specs.is_null() {
    return Vec::new();
  }
  // SAFETY: SDL returned `count` non-null spec pointers at `specs`; `i` is
  // below that, and each spec is copied out before the free.
  let result = (0..count.max(0) as usize).map(|i| unsafe { *(*specs.add(i)) }).collect();
  // SAFETY: `specs` is SDL's single allocation from
  // SDL_GetCameraSupportedFormats, freed once.
  unsafe { SDL_free(specs as *mut std::ffi::c_void) };
  result
}

/// The uncompressed native spec closest to the requested size (ties broken
/// by frame rate closest to `PREFERRED_FPS_MILLI`), or None when the camera
/// only offers compressed formats (e.g. MJPG-only at every size).
fn native_spec(id: u32, width: u32, height: u32) -> Option<SDL_CameraSpec> {
  let target = (width as i64) * (height as i64);
  supported_formats(id).into_iter().filter(|s| s.format != SDL_PIXELFORMAT_MJPG).min_by_key(|s| {
    let area = (s.width as i64) * (s.height as i64);
    let fps_milli = match s.framerate_denominator {
      0 => 0,
      d => 1000 * (s.framerate_numerator as i64) / (d as i64),
    };
    ((area - target).abs(), (fps_milli - PREFERRED_FPS_MILLI).abs())
  })
}

/// Where the OS permission for an open camera stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PermissionState {
  Pending,
  Approved,
  Denied,
}

/// An open camera device, closed on drop.
pub struct Camera {
  raw: *mut SDL_Camera,
}

// SAFETY: SDL documents the camera calls (permission, format, acquire,
// release, close) as callable from any thread, and one thread uses the
// device at a time (the UI thread until a producer takes it, the producer
// from then on). Not Sync: two threads at once is not that.
unsafe impl Send for Camera {}

impl Camera {
  /// Open camera `id` near `width` x `height`. Prefers an uncompressed
  /// native spec near the requested size; the caller converts frames to
  /// RGBA32 (`Frame::rgba`). Asking SDL for RGBA32 lets it pick the native
  /// format itself, and that choice can land on MJPG, whose stb-based
  /// decode emits green/grey garbage for the table-less MJPEG many UVC
  /// cameras produce. Only falls back to SDL-converted RGBA32 when the
  /// camera offers nothing but compressed formats. The subsystem must be
  /// up (`start` reports `Ready`). Opening raises the OS permission prompt
  /// where there is one (see `permission`).
  pub fn open(id: u32, width: u32, height: u32) -> Result<Camera, String> {
    let spec = native_spec(id, width, height).unwrap_or(SDL_CameraSpec {
      format: SDL_PIXELFORMAT_RGBA32,
      colorspace: SDL_COLORSPACE_SRGB,
      width: width as i32,
      height: height as i32,
      framerate_numerator: FALLBACK_FPS,
      framerate_denominator: 1,
    });
    // SAFETY: `spec` is a live, fully initialized spec SDL only reads.
    let raw = unsafe { SDL_OpenCamera(SDL_CameraID(id), &spec) };
    if raw.is_null() {
      return Err(format!("failed to open camera {id}: {}", crate::sdl_error()));
    }
    Ok(Camera { raw })
  }

  /// The OS permission's state; Pending until the user (or the backend)
  /// answers.
  pub fn permission(&self) -> PermissionState {
    // SAFETY: `raw` is an open camera, live until drop.
    match unsafe { SDL_GetCameraPermissionState(self.raw) } {
      SDL_CameraPermissionState::APPROVED => PermissionState::Approved,
      SDL_CameraPermissionState::DENIED => PermissionState::Denied,
      _ => PermissionState::Pending,
    }
  }

  /// The frame interval from the negotiated format (valid once permission
  /// is approved); None when SDL reports no rate.
  pub fn frame_period(&self) -> Option<Duration> {
    let mut spec = SDL_CameraSpec::default();
    // SAFETY: `raw` is an open camera, live until drop; `spec` is a live
    // out-parameter.
    if !unsafe { SDL_GetCameraFormat(self.raw, &mut spec) } {
      return None;
    }
    if spec.framerate_numerator <= 0 || spec.framerate_denominator <= 0 {
      return None;
    }
    Some(Duration::from_secs_f64(spec.framerate_denominator as f64 / spec.framerate_numerator as f64))
  }

  /// The oldest queued frame, or None when no new frame is available.
  /// Never blocks. The frame is released to SDL on drop; do not hold one
  /// across polls.
  pub fn acquire(&self) -> Option<Frame<'_>> {
    let mut timestamp_ns: u64 = 0;
    // SAFETY: `raw` is an open camera, live until drop; `timestamp_ns` is
    // a live out-parameter.
    let surface = unsafe { SDL_AcquireCameraFrame(self.raw, &mut timestamp_ns) };
    if surface.is_null() {
      return None;
    }
    Some(Frame { camera: self, surface, timestamp_ns })
  }
}

impl Drop for Camera {
  fn drop(&mut self) {
    // SAFETY: `raw` is an open camera, closed once here; no frame outlives
    // it (a Frame borrows the Camera).
    unsafe { SDL_CloseCamera(self.raw) };
  }
}

/// One acquired camera frame, released to SDL on drop.
pub struct Frame<'c> {
  camera: &'c Camera,
  surface: *mut SDL_Surface,
  timestamp_ns: u64,
}

impl Frame<'_> {
  fn surface(&self) -> &SDL_Surface {
    // SAFETY: `surface` is the non-null frame SDL handed out at acquire,
    // valid until it is released at drop.
    unsafe { &*self.surface }
  }

  pub fn width(&self) -> u32 {
    self.surface().w.max(0) as u32
  }

  pub fn height(&self) -> u32 {
    self.surface().h.max(0) as u32
  }

  /// SDL's capture timestamp, in nanoseconds on the tick timeline (see
  /// `crate::time::ticks_epoch`).
  pub fn timestamp_ns(&self) -> u64 {
    self.timestamp_ns
  }

  /// Degrees (clockwise, snapped to 0/90/180/270) to rotate the frame for
  /// an upright image; SDL sets this per frame on mobile (sensor
  /// orientation plus the current display rotation), absent means 0.
  pub fn rotation_degrees(&self) -> u32 {
    // SAFETY: `surface` is a live frame (see `surface`); the property
    // read copies out a float, with 0.0 when absent.
    let degrees = unsafe {
      let props = SDL_GetSurfaceProperties(self.surface);
      SDL_GetFloatProperty(props, SDL_PROP_SURFACE_ROTATION_FLOAT, 0.0)
    };
    let normalized = ((degrees.round() as i32 % 360) + 360) % 360;
    match normalized {
      90 | 180 | 270 => normalized as u32,
      0 => 0,
      other => {
        log::warn!("[ore::camera] non-quadrant frame rotation {other}, treating as 0");
        0
      }
    }
  }

  /// The frame's pixels as RGBA32 rows, with the row pitch in bytes. A
  /// frame already in RGBA32 is the surface's own pixels (pitch times
  /// height bytes, valid while the frame is); any other format is
  /// converted into `scratch` (resized to fit), tightly packed.
  pub fn rgba<'f>(&'f self, scratch: &'f mut Vec<u8>) -> Result<(&'f [u8], usize), String> {
    let surface = self.surface();
    let (width, height) = (self.width() as usize, self.height() as usize);
    if surface.format == SDL_PIXELFORMAT_RGBA32 {
      let pitch = surface.pitch.max(0) as usize;
      // SAFETY: an RGBA32 surface's pixels hold `pitch` bytes for each of
      // its `height` rows, valid until the frame is released at drop, which
      // the returned borrow of `self` cannot outlive.
      let rows = unsafe { std::slice::from_raw_parts(surface.pixels as *const u8, pitch * height) };
      return Ok((rows, pitch));
    }
    let row_bytes = width * RGBA_BYTES;
    scratch.resize(row_bytes * height, 0);
    // SAFETY: the source is the live surface's own pixels, format and
    // pitch; the destination holds `row_bytes` for each of `height` rows.
    let converted = unsafe {
      SDL_ConvertPixels(
        surface.w,
        surface.h,
        surface.format,
        surface.pixels,
        surface.pitch,
        SDL_PIXELFORMAT_RGBA32,
        scratch.as_mut_ptr() as *mut std::ffi::c_void,
        row_bytes as std::ffi::c_int,
      )
    };
    if !converted {
      return Err(format!("conversion failed: {}", crate::sdl_error()));
    }
    Ok((scratch.as_slice(), row_bytes))
  }
}

impl Drop for Frame<'_> {
  fn drop(&mut self) {
    // SAFETY: `surface` was acquired from this camera, which the borrow
    // keeps open, and is released once here.
    unsafe { SDL_ReleaseCameraFrame(self.camera.raw, self.surface) };
  }
}
