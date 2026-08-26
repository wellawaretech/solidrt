//! Camera capture via the SDL camera subsystem.
//!
//! A session opens an SDL camera at an uncompressed native format near the
//! requested size (frames are converted to RGBA32 in the pump) and exposes
//! them through a registry texture (see `Context::pump_cameras`), so a camera
//! view is just a texture draw for whatever sits on top. Opening triggers the
//! OS permission prompt; sessions start `Pending` and become `Ready` (texture
//! created at the delivered format), `Denied`, or `Failed` (deadlines, see
//! `INIT_DEADLINE` and `PENDING_DEADLINE`). A session opened while the camera
//! subsystem is still starting waits for it first: the pump opens the device
//! once the subsystem reports in. The pump is driven once per frame from the
//! UI thread; `SDL_AcquireCameraFrame` is non-blocking, so no camera thread
//! is needed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

use sdl3::sys::camera::{SDL_Camera, SDL_CameraPosition, SDL_CameraSpec};
use sdl3::sys::pixels::{SDL_COLORSPACE_SRGB, SDL_PIXELFORMAT_MJPG, SDL_PIXELFORMAT_RGBA32};

use crate::sdl_utils;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraFacing {
  Front,
  Back,
  Unknown,
}

pub struct CameraInfo {
  pub id: u32,
  pub name: String,
  pub facing: CameraFacing,
}

#[derive(Clone)]
pub enum CameraStatus {
  /// Waiting for the OS permission prompt (or, before that, for the camera
  /// subsystem to start).
  Pending,
  /// Streaming into `texture_id` at the delivered size.
  Ready {
    texture_id: u64,
    width: u32,
    height: u32,
  },
  Denied,
  /// The subsystem or the stream never started (see `INIT_DEADLINE` and
  /// `PENDING_DEADLINE`); the device is released and the message explains
  /// what happened.
  Failed(String),
}

/// A deferred open: the device is chosen and opened by the pump once the
/// subsystem is up (see `INIT_DEADLINE`).
struct OpenRequest {
  device: Option<u32>,
  facing: Option<CameraFacing>,
  size: Option<(u32, u32)>,
}

struct Session {
  /// Null while `request` is pending.
  camera: *mut SDL_Camera,
  status: CameraStatus,
  /// The open still to perform, for a session opened while the subsystem was
  /// starting.
  request: Option<OpenRequest>,
  /// When the session was opened (restarted when a deferred open gets its
  /// device); drives `INIT_DEADLINE` and the Linux `PENDING_DEADLINE`.
  opened_at: std::time::Instant,
  /// Permission granted; streaming starts and the texture is created on the
  /// first frame, since the upright size depends on the per-frame rotation.
  approved: bool,
  /// Registry id the stream texture is (re)created at, allocated up front so
  /// it survives size changes when the device rotates.
  texture_id: u64,
  /// Upright-frame buffer (pitch repack and/or rotation output).
  scratch: Vec<u8>,
  /// RGBA32 conversion buffer for frames delivered in a native format.
  convert: Vec<u8>,
  /// Decode QR codes from the stream (opt-in at open).
  scan_qr: bool,
  /// Greyscale scratch for the QR decoder.
  gray: Vec<u8>,
  /// Uploaded frames since the last decode attempt (decode every Nth frame).
  frames_since_scan: u32,
  /// Last time a decode was reported, for the 1s re-report throttle.
  last_emit: Option<std::time::Instant>,
  /// Decoded payloads waiting for `take_camera_barcodes`.
  barcodes: Vec<String>,
}

/// Try a QR decode every Nth uploaded frame (~3/s at 30fps); decoding costs
/// around a millisecond per attempt (several when a code is present) and runs
/// on the UI thread.
const SCAN_INTERVAL_FRAMES: u32 = 10;

/// How long a session may sit `Pending` before it is failed and the device
/// released. Linux only: neither Linux backend ever shows a consent prompt, so
/// Pending this long is always a wedged backend, not a user deciding - SDL's
/// pipewire backend reports permission only when the pw stream starts
/// streaming and silently swallows the stream ERROR state, so a failed
/// negotiation stays PENDING forever (observed with a working webcam,
/// 2026-08-01). Without the deadline the open() promise never settles, the JS
/// side has no handle to close, and the device stays held until the engine
/// exits ("Camera already opened" on every retry). On Android/macOS/Windows
/// Pending legitimately means the OS permission dialog is up, so no deadline
/// there.
#[cfg(target_os = "linux")]
const PENDING_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// How long an open may wait for the camera subsystem to come up before the
/// session is failed. Every platform, unlike `PENDING_DEADLINE`: init never
/// waits on the user (no platform prompts at init, only at open), so past
/// this it is a wedged backend - on a Raspberry Pi 4 the v4l2 backend never
/// returns (see `sdl_utils::camera_subsystem_init`). A healthy init takes
/// well under a second, so on every other machine the first open simply
/// works instead of failing with "starting" and needing a retry.
const INIT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

impl Session {
  /// Release the device; a deferred open that never got one has nothing to
  /// close.
  fn close_device(&self) {
    if !self.camera.is_null() {
      sdl_utils::camera_close(self.camera);
    }
  }
}

#[derive(Default)]
pub struct CameraRegistry {
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
}

enum InitState {
  NotStarted,
  Starting,
  Done(Result<(), String>),
}

static INIT_STATE: Mutex<InitState> = Mutex::new(InitState::NotStarted);

/// What `ensure_init` reports. `Starting` is the normal first-call state.
enum InitStatus {
  Ready,
  Starting,
  Failed(String),
}

/// Lazy one-time SDL camera subsystem init (first list/open call). Never
/// blocks: the first call spawns a worker and reports `Starting`; a list
/// treats that as "no cameras yet", an open waits for it (`pump_init`). When
/// the worker finishes, the devices it found arrive as SDL
/// CAMERA_DEVICE_ADDED events through the normal hotplug path
/// (translate_event -> CameraDeviceChange), so listeners re-enumerate
/// without polling.
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
/// until an app actually wants a camera - which is why the launcher no longer
/// enumerates cameras to decide whether to show its scan button.
///
/// No other SDL_InitSubSystem call may race this one (SDL's init refcounting
/// is not thread-safe). Startup order is not the guarantee it looks like:
/// gamepad init is itself a udev scan and can still be running when the
/// first script calls in, which is exactly the overlap above.
fn ensure_init() -> InitStatus {
  let mut state = INIT_STATE.lock().expect("camera init state poisoned");
  match &*state {
    InitState::NotStarted => {
      let spawned = std::thread::Builder::new().name("srt-camera-init".into()).spawn(|| {
        let result = if sdl_utils::camera_subsystem_init() {
          Ok(())
        } else {
          // SDL's error string is thread-local: capture it here, on the
          // thread the init actually ran on.
          Err(format!("camera subsystem init failed: {}", sdl_utils::sdl_error()))
        };
        if let Err(e) = &result {
          log::warn!("[camera] {e}");
        }
        *INIT_STATE.lock().expect("camera init state poisoned") = InitState::Done(result);
      });
      match spawned {
        Ok(_) => {
          *state = InitState::Starting;
          InitStatus::Starting
        }
        Err(e) => {
          let msg = format!("camera init thread failed to spawn: {e}");
          *state = InitState::Done(Err(msg.clone()));
          InitStatus::Failed(msg)
        }
      }
    }
    InitState::Starting => InitStatus::Starting,
    InitState::Done(Ok(())) => InitStatus::Ready,
    InitState::Done(Err(e)) => InitStatus::Failed(e.clone()),
  }
}

/// The uncompressed native spec closest to the requested size (ties broken by
/// framerate closest to 30), or None when the camera only offers compressed
/// formats (e.g. MJPG-only at every size).
fn native_spec(id: u32, width: u32, height: u32) -> Option<SDL_CameraSpec> {
  let target = (width as i64) * (height as i64);
  sdl_utils::camera_supported_formats(id).into_iter().filter(|s| s.format != SDL_PIXELFORMAT_MJPG).min_by_key(|s| {
    let area = (s.width as i64) * (s.height as i64);
    let fps_milli = match s.framerate_denominator {
      0 => 0,
      d => 1000 * (s.framerate_numerator as i64) / (d as i64),
    };
    ((area - target).abs(), (fps_milli - 30_000).abs())
  })
}

fn facing_of(position: SDL_CameraPosition) -> CameraFacing {
  match position {
    SDL_CameraPosition::FRONT_FACING => CameraFacing::Front,
    SDL_CameraPosition::BACK_FACING => CameraFacing::Back,
    _ => CameraFacing::Unknown,
  }
}

pub fn list_cameras() -> Vec<CameraInfo> {
  // "Starting" is the normal first-call state (a real failure already warned
  // from the init worker); the list refreshes via CameraDeviceChange when the
  // subsystem comes up.
  match ensure_init() {
    InitStatus::Ready => {}
    InitStatus::Starting => return Vec::new(),
    InitStatus::Failed(e) => {
      log::debug!("[camera] list unavailable: {e}");
      return Vec::new();
    }
  }
  sdl_utils::camera_ids()
    .into_iter()
    .map(|id| CameraInfo { id, name: sdl_utils::camera_name(id), facing: facing_of(sdl_utils::camera_position(id)) })
    .collect()
}

/// Choose the device for `req` and open it: an explicit camera id, otherwise
/// the first camera matching `facing` (or simply the first one). The
/// subsystem must be up.
fn open_device(req: &OpenRequest) -> Result<*mut SDL_Camera, String> {
  let id = match req.device {
    Some(d) => d,
    None => {
      let cams = list_cameras();
      // Default to the back camera: viewfinders and scanning overwhelmingly
      // want it on phones, and desktop webcams (facing Unknown) fall through
      // to "first camera" either way.
      let want = req.facing.unwrap_or(CameraFacing::Back);
      let preferred = cams.iter().find(|c| c.facing == want);
      preferred.or_else(|| cams.first()).map(|c| c.id).ok_or_else(|| "no cameras available".to_string())?
    }
  };

  // Prefer an uncompressed native spec near the requested size; the pump
  // converts frames to RGBA32. Asking SDL for RGBA32 lets it pick the
  // native format itself, and that choice can land on MJPG, whose
  // stb-based decode emits green/grey garbage for the table-less MJPEG
  // many UVC cameras produce. Only fall back to SDL-converted RGBA32 when
  // the camera offers nothing but compressed formats.
  let (width, height) = req.size.unwrap_or((640, 480));
  let spec = native_spec(id, width, height).unwrap_or(SDL_CameraSpec {
    format: SDL_PIXELFORMAT_RGBA32,
    colorspace: SDL_COLORSPACE_SRGB,
    width: width as i32,
    height: height as i32,
    framerate_numerator: 30,
    framerate_denominator: 1,
  });
  let camera = sdl_utils::camera_open(id, &spec);
  if camera.is_null() {
    return Err(format!("failed to open camera {id}: {}", sdl_utils::sdl_error()));
  }
  Ok(camera)
}

impl crate::context::Context {
  /// Open a camera session (see `open_device` for the device choice). Returns
  /// the session id; the session is `Pending` until the OS permission
  /// resolves. While the subsystem is still starting the open is deferred:
  /// the pump performs it once the subsystem reports in, or fails the session
  /// at `INIT_DEADLINE`, so a first open never has to be retried.
  pub fn open_camera(
    &self,
    device: Option<u32>,
    facing: Option<CameraFacing>,
    size: Option<(u32, u32)>,
    scan_qr: bool,
  ) -> Result<u64, String> {
    let request = OpenRequest { device, facing, size };
    let (camera, request) = match ensure_init() {
      InitStatus::Ready => (open_device(&request)?, None),
      InitStatus::Starting => (std::ptr::null_mut(), Some(request)),
      InitStatus::Failed(e) => return Err(e),
    };

    let sid = {
      let mut next = self.cameras.next_id.borrow_mut();
      *next += 1;
      *next
    };
    self.cameras.sessions.borrow_mut().insert(
      sid,
      Session {
        camera,
        status: CameraStatus::Pending,
        request,
        opened_at: std::time::Instant::now(),
        approved: false,
        texture_id: self.borrow_texture_id(),
        scratch: Vec::new(),
        convert: Vec::new(),
        scan_qr,
        gray: Vec::new(),
        frames_since_scan: 0,
        last_emit: None,
        barcodes: Vec::new(),
      },
    );
    Ok(sid)
  }

  /// Drain the QR payloads decoded since the last call.
  pub fn take_camera_barcodes(&self, sid: u64) -> Vec<String> {
    match self.cameras.sessions.borrow_mut().get_mut(&sid) {
      Some(session) => std::mem::take(&mut session.barcodes),
      None => Vec::new(),
    }
  }

  pub fn camera_status(&self, sid: u64) -> Option<CameraStatus> {
    self.cameras.sessions.borrow().get(&sid).map(|s| s.status.clone())
  }

  /// Close the session and release the device and its texture id. The id
  /// takes the deferred-destroy path, so a `<texture>` still showing it
  /// keeps the last frame until it lets go.
  pub fn close_camera(&self, sid: u64) {
    if let Some(session) = self.cameras.sessions.borrow_mut().remove(&sid) {
      session.close_device();
      self.release_borrowed(session.texture_id);
    }
  }

  /// Release every open camera. Called between engine runs so a reloaded app
  /// never inherits (or leaks) a live capture device.
  pub fn close_all_cameras(&self) {
    for (_, session) in self.cameras.sessions.borrow_mut().drain() {
      session.close_device();
      self.release_borrowed(session.texture_id);
    }
  }

  /// Advance all sessions: resolve pending permission prompts and upload the
  /// latest frame of each ready session into its texture. Run once per frame
  /// on the UI thread; does nothing when no sessions are open. Returns true
  /// when at least one texture received a new frame, so callers can schedule
  /// a redraw.
  pub fn pump_cameras(&self) -> bool {
    let mut sessions = self.cameras.sessions.borrow_mut();
    let mut uploaded = false;
    for session in sessions.values_mut() {
      match session.status {
        CameraStatus::Pending if session.request.is_some() => Self::pump_init(session),
        CameraStatus::Pending => {
          if !session.approved {
            Self::pump_permission(session);
          }
          if session.approved {
            uploaded |= self.pump_frame(session);
          }
          // Still Pending covers both "no permission" and "approved but no
          // first frame": either way the backend never delivered.
          #[cfg(target_os = "linux")]
          if matches!(session.status, CameraStatus::Pending) && session.opened_at.elapsed() >= PENDING_DEADLINE {
            log::warn!(
              "[camera] start timed out after {}s (backend delivered neither permission nor a frame); closing",
              PENDING_DEADLINE.as_secs()
            );
            sdl_utils::camera_close(session.camera);
            session.status = CameraStatus::Failed("camera start timed out".to_string());
          }
        }
        CameraStatus::Ready { .. } => uploaded |= self.pump_frame(session),
        CameraStatus::Denied | CameraStatus::Failed(_) => {}
      }
    }
    uploaded
  }

  /// Finish a deferred open: once the subsystem is up, choose and open the
  /// device (the session then proceeds like a direct open, its `opened_at`
  /// restarted for `PENDING_DEADLINE`). An init failure, an open failure or
  /// `INIT_DEADLINE` fails the session instead.
  fn pump_init(session: &mut Session) {
    let Some(request) = &session.request else {
      return;
    };
    let outcome = match ensure_init() {
      InitStatus::Ready => open_device(request),
      InitStatus::Failed(e) => Err(e),
      InitStatus::Starting if session.opened_at.elapsed() >= INIT_DEADLINE => {
        log::warn!("[camera] subsystem did not start within {}s; failing the open", INIT_DEADLINE.as_secs());
        Err(format!("camera subsystem did not start within {}s", INIT_DEADLINE.as_secs()))
      }
      InitStatus::Starting => return,
    };
    session.request = None;
    match outcome {
      Ok(camera) => {
        session.camera = camera;
        session.opened_at = std::time::Instant::now();
      }
      Err(e) => session.status = CameraStatus::Failed(e),
    }
  }

  fn pump_permission(session: &mut Session) {
    use sdl3::sys::camera::SDL_CameraPermissionState as P;
    match sdl_utils::camera_permission(session.camera) {
      P::APPROVED => session.approved = true,
      P::DENIED => {
        log::warn!("[camera] permission denied");
        sdl_utils::camera_close(session.camera);
        session.status = CameraStatus::Denied;
      }
      _ => {}
    }
  }

  /// Upload the latest frame, rotated upright per SDL's per-frame rotation
  /// (mobile sensor + display orientation). The texture is created on the
  /// first frame and recreated at the same id when the upright size changes
  /// (device rotated between portrait and landscape). Returns true when a
  /// frame was uploaded.
  fn pump_frame(&self, session: &mut Session) -> bool {
    let frame = sdl_utils::camera_acquire_frame(session.camera);
    if frame.is_null() {
      return false;
    }
    let surface = unsafe { &*frame };
    let (frame_w, frame_h) = (surface.w as u32, surface.h as u32);
    let rotation = sdl_utils::surface_rotation_degrees(frame);
    let (width, height) = if rotation == 90 || rotation == 270 { (frame_h, frame_w) } else { (frame_w, frame_h) };

    let pitch = surface.pitch as usize;
    let row_bytes = (frame_w as usize) * 4;
    let rgba_src = surface.format == SDL_PIXELFORMAT_RGBA32;
    if !rgba_src {
      // Camera opened at a native format (e.g. YUY2); convert to RGBA32.
      session.convert.resize(row_bytes * frame_h as usize, 0);
      if !sdl_utils::surface_to_rgba(surface, &mut session.convert) {
        log::warn!("[camera] frame conversion failed: {}", sdl_utils::sdl_error());
        sdl_utils::camera_release_frame(session.camera, frame);
        return false;
      }
    }
    let pixels: &[u8] = if rgba_src && rotation == 0 && pitch == row_bytes {
      unsafe { std::slice::from_raw_parts(surface.pixels as *const u8, row_bytes * frame_h as usize) }
    } else if !rgba_src && rotation == 0 {
      &session.convert
    } else {
      let (src, src_pitch) =
        if rgba_src { (surface.pixels as *const u8, pitch) } else { (session.convert.as_ptr(), row_bytes) };
      upright_into(src, src_pitch, frame_w as usize, frame_h as usize, rotation, &mut session.scratch);
      &session.scratch
    };

    let recreate = match session.status {
      CameraStatus::Ready { width: w, height: h, .. } => w != width || h != height,
      _ => true,
    };
    if recreate {
      if let Err(e) = self.create_texture_at(
        session.texture_id,
        width,
        height,
        pixels,
        Default::default(),
        crate::gpu::TextureFormat::Rgba8,
        None,
      ) {
        log::warn!("[camera] frame texture create failed: {e}");
        sdl_utils::camera_release_frame(session.camera, frame);
        return false;
      }
      log::info!("[camera] streaming {width}x{height} (rotation {rotation}) -> texture {}", session.texture_id);
      session.status = CameraStatus::Ready { texture_id: session.texture_id, width, height };
    } else {
      let _ = self.update_texture(session.texture_id, pixels, 0);
    }

    if session.scan_qr {
      session.frames_since_scan += 1;
      if session.frames_since_scan >= SCAN_INTERVAL_FRAMES {
        session.frames_since_scan = 0;
        crate::barcode::to_greyscale(pixels, &mut session.gray);
        if let Some(content) = crate::barcode::decode_qr(&session.gray, width, height) {
          // Re-report the (still visible) code at most once a second, matching
          // typical scanner behavior; consumers connect/dedupe on their side.
          let due = session.last_emit.map_or(true, |t| t.elapsed() >= std::time::Duration::from_secs(1));
          if due {
            session.last_emit = Some(std::time::Instant::now());
            session.barcodes.push(content);
          }
        }
      }
    }
    sdl_utils::camera_release_frame(session.camera, frame);
    true
  }
}

/// Copy a `pitch`-strided RGBA8 image into `dst` rotated clockwise by
/// `rotation` degrees, tightly packed (also flattens any pitch padding).
fn upright_into(base: *const u8, pitch: usize, w: usize, h: usize, rotation: u32, dst: &mut Vec<u8>) {
  dst.resize(w * h * 4, 0);
  for r in 0..h {
    let row = unsafe { std::slice::from_raw_parts(base.add(r * pitch), w * 4) };
    match rotation {
      // (r, c) -> (c, h-1-r); rotated row length is h
      90 => {
        for c in 0..w {
          let di = (c * h + (h - 1 - r)) * 4;
          dst[di..di + 4].copy_from_slice(&row[c * 4..c * 4 + 4]);
        }
      }
      // (r, c) -> (h-1-r, w-1-c)
      180 => {
        for c in 0..w {
          let di = ((h - 1 - r) * w + (w - 1 - c)) * 4;
          dst[di..di + 4].copy_from_slice(&row[c * 4..c * 4 + 4]);
        }
      }
      // (r, c) -> (w-1-c, r); rotated row length is h
      270 => {
        for c in 0..w {
          let di = ((w - 1 - c) * h + r) * 4;
          dst[di..di + 4].copy_from_slice(&row[c * 4..c * 4 + 4]);
        }
      }
      // Pitch repack only.
      _ => dst[r * w * 4..(r + 1) * w * 4].copy_from_slice(row),
    }
  }
}
