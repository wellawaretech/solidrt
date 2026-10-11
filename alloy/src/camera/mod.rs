//! Camera capture via the SDL camera subsystem.
//!
//! A session opens an SDL camera at an uncompressed native format near the
//! requested size and streams it two ways from one slot: the session's
//! `FrameTap`, filled by a camera thread of its own (`producer`) with every
//! frame converted to RGBA32 and rotated upright, and the registry texture
//! the raster thread uploads that slot into at each frame, so a camera view
//! is just a texture draw for whatever sits on top, and a module reads the
//! same frames on its own thread (`Context::camera_frame_tap`). Opening
//! triggers the OS permission prompt; sessions start `Pending` and become
//! `Ready` (texture created at the first frame's upright size), `Denied`, or
//! `Failed` (deadlines, see `INIT_DEADLINE` and `PENDING_DEADLINE`). A
//! session opened while the camera subsystem is still starting waits for it
//! first: the pump opens the device once the subsystem reports in. The pump
//! runs once per frame on the UI thread and does no pixel work: it polls the
//! permission while Pending, starts the producer once approved, and from
//! then on reads the slot's seq and size to note content and resize the
//! texture. The device never signals: it is polled, so a waiter on an open
//! pumps its one session between frames too (`pump_camera`), on its own
//! cadence, and the answer does not wait for a frame.
//!
//! A test installs a still image as the camera (`install_still_camera`):
//! from then on the device list is that one camera, every open shows the
//! image, approved at once with no SDL subsystem, and a later install
//! publishes the new image into every open session. No thread runs for it
//! and it demands no frames, so an app test settles.

pub(crate) mod producer;
mod tap;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use sdl3::sys::camera::{SDL_Camera, SDL_CameraPosition, SDL_CameraSpec};
use sdl3::sys::pixels::{SDL_COLORSPACE_SRGB, SDL_PIXELFORMAT_MJPG, SDL_PIXELFORMAT_RGBA32};

use crate::sdl_utils;

pub use tap::{CameraFrame, FrameTap, TapWait};

/// The device id of the still camera a test installs: never an SDL id
/// (those start at 1).
pub const STILL_CAMERA_ID: u32 = 0;

/// The image a test installed as the camera (see `install_still_camera`).
struct StillFrame {
  width: u32,
  height: u32,
  pixels: Vec<u8>,
}

/// What fills a session's tap once it is approved.
enum Producer {
  /// The `sol-camera` thread over an SDL device (see producer.rs).
  Device(JoinHandle<()>),
  /// A test's still image, published into the tap at open and at every
  /// later install.
  Still,
}

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
  /// Null while `request` is pending, and null again once the producer
  /// owns the device.
  camera: *mut SDL_Camera,
  status: CameraStatus,
  /// The open still to perform, for a session opened while the subsystem was
  /// starting.
  request: Option<OpenRequest>,
  /// When the session was opened (restarted when a deferred open gets its
  /// device); drives `INIT_DEADLINE` and the Linux `PENDING_DEADLINE`.
  opened_at: std::time::Instant,
  /// Permission granted; the producer starts and the texture is created on
  /// the first frame, since the upright size depends on the per-frame
  /// rotation.
  approved: bool,
  /// Registry id the stream texture is (re)created at, allocated up front so
  /// it survives size changes when the device rotates.
  texture_id: u64,
  /// The session's frame slot: the producer fills it, the raster thread and
  /// every tap holder read it.
  tap: FrameTap,
  /// The tap's producer, from the permission grant to the close.
  producer: Option<Producer>,
  /// The newest frame seq the UI side has acted on (texture created or
  /// content noted).
  noted_seq: u64,
  /// The QR scanner over the tap, shared with its thread; None for a
  /// session opened without scan.
  scanner: Option<Arc<crate::barcode::StreamScanner>>,
}

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
  /// Release the device and end the session's threads: close the tap (every
  /// waiter wakes with `Closed`, the scanner ends on its own) and join the
  /// producer, which closes the device on its way out; a device the producer
  /// never took is closed here. A deferred open that never got one has
  /// nothing to close. Idempotent.
  fn stop(&mut self) {
    self.tap.close();
    match self.producer.take() {
      Some(Producer::Device(thread)) => {
        if thread.join().is_err() {
          log::warn!("[camera] camera thread ended in a panic");
        }
      }
      Some(Producer::Still) => {}
      None => {
        if !self.camera.is_null() {
          sdl_utils::camera_close(self.camera);
        }
      }
    }
    self.camera = std::ptr::null_mut();
  }
}

impl Drop for Session {
  fn drop(&mut self) {
    self.stop();
  }
}

#[derive(Default)]
pub struct CameraRegistry {
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
  /// The still image standing in for every camera while a test has one
  /// installed.
  still: RefCell<Option<Arc<StillFrame>>>,
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
/// until an app actually wants a camera - which is why the player no longer
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
      let spawned = std::thread::Builder::new().name("sol-camera-init".into()).spawn(|| {
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

fn list_devices() -> Vec<CameraInfo> {
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
      let cams = list_devices();
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
  /// The camera devices: the SDL list (the first call also starts the
  /// subsystem, so expect an empty list until the device events arrive),
  /// or the one still camera while a test has one installed.
  pub fn list_cameras(&self) -> Vec<CameraInfo> {
    if self.cameras.still.borrow().is_some() {
      return vec![CameraInfo { id: STILL_CAMERA_ID, name: "test camera".to_string(), facing: CameraFacing::Unknown }];
    }
    list_devices()
  }

  /// Install `pixels` (RGBA8, `width` x `height`, tightly packed) as the
  /// camera, for a test: from now on every open shows this image, and every
  /// open still session shows it from its next frame. Errs on a size that
  /// does not match the pixels.
  pub fn install_still_camera(&self, width: u32, height: u32, pixels: Vec<u8>) -> Result<(), String> {
    let expected = (width as usize) * (height as usize) * 4;
    if width == 0 || height == 0 || pixels.len() != expected {
      return Err(format!("{width}x{height} RGBA8 is {expected} bytes, got {}", pixels.len()));
    }
    let still = Arc::new(StillFrame { width, height, pixels });
    for session in self.cameras.sessions.borrow().values() {
      if matches!(session.producer, Some(Producer::Still)) {
        session.tap.publish(0, still.width, still.height, still.pixels.clone());
      }
    }
    *self.cameras.still.borrow_mut() = Some(still);
    Ok(())
  }

  /// Open a camera session (see `open_device` for the device choice). Returns
  /// the session id; the session is `Pending` until the OS permission
  /// resolves. While the subsystem is still starting the open is deferred:
  /// the pump performs it once the subsystem reports in, or fails the session
  /// at `INIT_DEADLINE`, so a first open never has to be retried. With a
  /// still camera installed the session is the still, approved at once.
  pub fn open_camera(
    &self,
    device: Option<u32>,
    facing: Option<CameraFacing>,
    size: Option<(u32, u32)>,
    scan_qr: bool,
  ) -> Result<u64, String> {
    let still = self.cameras.still.borrow().clone();
    let (camera, request, producer) = match still {
      Some(_) => (std::ptr::null_mut(), None, Some(Producer::Still)),
      None => {
        let request = OpenRequest { device, facing, size };
        match ensure_init() {
          InitStatus::Ready => (open_device(&request)?, None, None),
          InitStatus::Starting => (std::ptr::null_mut(), Some(request), None),
          InitStatus::Failed(e) => return Err(e),
        }
      }
    };

    let sid = {
      let mut next = self.cameras.next_id.borrow_mut();
      *next += 1;
      *next
    };
    let tap = FrameTap::new();
    if let Some(still) = &still {
      tap.publish(0, still.width, still.height, still.pixels.clone());
    }
    let scanner = scan_qr.then(|| crate::barcode::spawn_stream_scanner(tap.clone()));
    self.cameras.sessions.borrow_mut().insert(
      sid,
      Session {
        camera,
        status: CameraStatus::Pending,
        request,
        opened_at: std::time::Instant::now(),
        approved: still.is_some(),
        texture_id: self.borrow_texture_id(),
        tap,
        producer,
        noted_seq: 0,
        scanner,
      },
    );
    Ok(sid)
  }

  /// Drain the QR payloads decoded since the last call.
  pub fn take_camera_barcodes(&self, sid: u64) -> Vec<String> {
    let sessions = self.cameras.sessions.borrow();
    match sessions.get(&sid).and_then(|session| session.scanner.as_ref()) {
      Some(scanner) => scanner.take(),
      None => Vec::new(),
    }
  }

  /// Whether the scanning session `sid` holds a frame its scanner has not
  /// examined yet, so a payload may still be on its way: what a settle
  /// waits on. None for a closed session or one opened without scan.
  pub fn camera_scan_pending(&self, sid: u64) -> Option<bool> {
    let sessions = self.cameras.sessions.borrow();
    let session = sessions.get(&sid)?;
    let scanner = session.scanner.as_ref()?;
    Some(scanner.pending(&session.tap))
  }

  pub fn camera_status(&self, sid: u64) -> Option<CameraStatus> {
    self.cameras.sessions.borrow().get(&sid).map(|s| s.status.clone())
  }

  /// A tap on the open camera streaming into `texture_id` (the id JS holds
  /// as `cam.texture`); None when no open session owns that id. Usable from
  /// any thread; it reports `Closed` once the session closes.
  pub fn camera_frame_tap(&self, texture_id: u64) -> Option<FrameTap> {
    self.cameras.sessions.borrow().values().find(|s| s.texture_id == texture_id).map(|s| s.tap.clone())
  }

  /// Whether a camera is streaming: standing frame demand like a playing
  /// video (`streaming_textures`), so the loop ticks on the refresh grid
  /// and the raster take sees every frame within one refresh.
  pub fn camera_streaming(&self) -> bool {
    self.cameras.sessions.borrow().values().any(|s| matches!(s.producer, Some(Producer::Device(_))))
  }

  /// Close the session: end its threads, release the device and its texture
  /// id. The id takes the deferred-destroy path, so a `<texture>` still
  /// showing it keeps the last frame until it lets go.
  pub fn close_camera(&self, sid: u64) {
    if let Some(session) = self.cameras.sessions.borrow_mut().remove(&sid) {
      let texture_id = session.texture_id;
      drop(session);
      self.release_borrowed(texture_id);
    }
  }

  /// Advance all sessions: resolve pending permission prompts, start the
  /// producer of each approved session, and note each new frame on its
  /// texture. Run once per frame on the UI thread; does nothing when no
  /// sessions are open. Returns true when at least one texture received a
  /// new frame, so callers can schedule a redraw.
  pub fn pump_cameras(&self) -> bool {
    let mut sessions = self.cameras.sessions.borrow_mut();
    let mut noted = false;
    for session in sessions.values_mut() {
      noted |= self.pump_session(session);
    }
    noted
  }

  /// Advance one session, for a waiter on its open polling between frames:
  /// the same step the per-frame pump takes, so a session leaves `Pending`
  /// (Ready, Denied or Failed) on the waiter's cadence on a host that runs
  /// no frame while the open is in flight. The two pumps interleave freely
  /// on the UI thread. Returns the status after the step; None for a closed
  /// session.
  pub fn pump_camera(&self, sid: u64) -> Option<CameraStatus> {
    let mut sessions = self.cameras.sessions.borrow_mut();
    let session = sessions.get_mut(&sid)?;
    self.pump_session(session);
    Some(session.status.clone())
  }

  /// One session's step: a deferred open once the subsystem is up, the
  /// permission answer and then the producer start while Pending, the
  /// slot's newest frame from then on. Returns true when a new frame was
  /// noted on the texture.
  fn pump_session(&self, session: &mut Session) -> bool {
    match session.status {
      CameraStatus::Pending if session.request.is_some() => {
        Self::pump_init(session);
        false
      }
      CameraStatus::Pending => {
        if !session.approved {
          Self::pump_permission(session);
        }
        if session.approved && session.producer.is_none() {
          Self::start_producer(session);
        }
        let noted = session.producer.is_some() && self.pump_tap(session);
        // Still Pending covers both "no permission" and "approved but no
        // first frame": either way the backend never delivered.
        #[cfg(target_os = "linux")]
        if matches!(session.status, CameraStatus::Pending) && session.opened_at.elapsed() >= PENDING_DEADLINE {
          log::warn!(
            "[camera] start timed out after {}s (backend delivered neither permission nor a frame); closing",
            PENDING_DEADLINE.as_secs()
          );
          session.stop();
          session.status = CameraStatus::Failed("camera start timed out".to_string());
        }
        noted
      }
      CameraStatus::Ready { .. } => self.pump_tap(session),
      CameraStatus::Denied | CameraStatus::Failed(_) => false,
    }
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
        session.stop();
        session.status = CameraStatus::Denied;
      }
      _ => {}
    }
  }

  /// Hand the device to the camera thread. A thread that cannot start fails
  /// the session; the device is released with it.
  fn start_producer(session: &mut Session) {
    match producer::spawn(session.camera, session.tap.clone()) {
      Ok(handle) => {
        session.camera = std::ptr::null_mut();
        session.producer = Some(Producer::Device(handle));
      }
      Err(e) => {
        log::warn!("[camera] {e}");
        session.stop();
        session.status = CameraStatus::Failed(e);
      }
    }
  }

  /// The UI side's per-frame read of the slot: a new size creates the
  /// texture (the first frame) or recreates it at the same id (the device
  /// rotated between portrait and landscape), storage only, and attaches
  /// the tap to it, so the raster thread uploads this frame and every
  /// later one itself at its next `Frame`, before anything draws the
  /// texture; a new seq at the same size notes content on the texture.
  /// Returns true when a new frame was noted.
  fn pump_tap(&self, session: &mut Session) -> bool {
    let Some(frame) = session.tap.latest() else {
      return false;
    };
    if frame.seq <= session.noted_seq {
      return false;
    }
    session.noted_seq = frame.seq;
    let recreate = match session.status {
      CameraStatus::Ready { width, height, .. } => width != frame.width || height != frame.height,
      _ => true,
    };
    if recreate {
      if let Err(e) = self.create_texture_at(
        session.texture_id,
        frame.width,
        frame.height,
        &[],
        Default::default(),
        crate::gpu::TextureFormat::Rgba8,
        None,
      ) {
        log::warn!("[camera] frame texture create failed: {e}");
        return false;
      }
      self.attach_camera_tap(session.texture_id, session.tap.clone(), frame.seq - 1);
      self.note_content(session.texture_id);
      log::info!("[camera] streaming {}x{} -> texture {}", frame.width, frame.height, session.texture_id);
      session.status = CameraStatus::Ready { texture_id: session.texture_id, width: frame.width, height: frame.height };
    } else {
      self.note_content(session.texture_id);
    }
    true
  }
}
