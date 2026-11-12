//! Camera capture over ore's camera facility (`ore::camera`).
//!
//! A session opens a camera through ore, which picks an uncompressed native
//! format near the requested size, and streams it two ways from one slot: the session's
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
//! image, approved at once with no camera subsystem, and a later install
//! publishes the new image into every open session. No thread runs for it
//! and it demands no frames, so an app test settles.

pub(crate) mod producer;
mod tap;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::JoinHandle;

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
  /// The `sol-camera` thread over a camera device (see producer.rs).
  Device(JoinHandle<()>),
  /// A test's still image, published into the tap at open and at every
  /// later install.
  Still,
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
  facing: Option<ore::camera::Facing>,
  size: Option<(u32, u32)>,
}

struct Session {
  /// None while `request` is pending, and None again once the producer
  /// owns the device.
  camera: Option<ore::camera::Camera>,
  status: CameraStatus,
  /// The open still to perform, for a session opened while the subsystem was
  /// starting.
  request: Option<OpenRequest>,
  /// When the session was opened (restarted when a deferred open gets its
  /// device); drives `INIT_DEADLINE` and `PENDING_DEADLINE`.
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
/// released. Applied only where the backend shows no consent prompt
/// (`ore::camera::permission_prompts()` false), since there a long Pending is
/// a wedged backend, not a user deciding. Without the deadline the open()
/// promise never settles, the JS side has no handle to close, and the device
/// stays held until the engine exits ("Camera already opened" on every
/// retry).
const PENDING_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

/// How long an open may wait for the camera subsystem to come up before the
/// session is failed. Every backend, unlike `PENDING_DEADLINE`: init never
/// waits on the user (no platform prompts at init, only at open), so past
/// this it is a wedged backend - on a Raspberry Pi 4 the v4l2 backend never
/// returns (see `ore::camera::start`). A healthy init takes
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
      None => {}
    }
    self.camera = None;
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

/// The size a camera opens at when the open names none: VGA, a size nearly
/// every camera offers natively, and cheap to convert and rotate per frame.
const DEFAULT_SIZE: (u32, u32) = (640, 480);

fn list_devices() -> Vec<ore::camera::CameraInfo> {
  // "Starting" is the normal first-call state (a real failure already warned
  // from the init worker); the list refreshes via CameraDeviceChange when the
  // subsystem comes up.
  match ore::camera::start() {
    ore::camera::Start::Ready => ore::camera::devices(),
    ore::camera::Start::Starting => Vec::new(),
    ore::camera::Start::Failed(e) => {
      log::debug!("[camera] list unavailable: {e}");
      Vec::new()
    }
  }
}

/// Choose the device for `req` and open it: an explicit camera id, otherwise
/// the first camera matching `facing` (or simply the first one). The
/// subsystem must be up.
fn open_device(req: &OpenRequest) -> Result<ore::camera::Camera, String> {
  let id = match req.device {
    Some(d) => d,
    None => {
      let cams = list_devices();
      // Default to the back camera: viewfinders and scanning overwhelmingly
      // want it on phones, and desktop webcams (facing Unknown) fall through
      // to "first camera" either way.
      let want = req.facing.unwrap_or(ore::camera::Facing::Back);
      let preferred = cams.iter().find(|c| c.facing == want);
      preferred.or_else(|| cams.first()).map(|c| c.id).ok_or_else(|| "no cameras available".to_string())?
    }
  };

  // ore picks the spec: an uncompressed native format near this size.
  let (width, height) = req.size.unwrap_or(DEFAULT_SIZE);
  ore::camera::Camera::open(id, width, height)
}

impl crate::context::Context {
  /// The camera devices: ore's list (the first call also starts the
  /// subsystem, so expect an empty list until the device events arrive),
  /// or the one still camera while a test has one installed.
  pub fn list_cameras(&self) -> Vec<ore::camera::CameraInfo> {
    if self.cameras.still.borrow().is_some() {
      return vec![ore::camera::CameraInfo {
        id: STILL_CAMERA_ID,
        name: "test camera".into(),
        facing: ore::camera::Facing::Unknown,
      }];
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
    facing: Option<ore::camera::Facing>,
    size: Option<(u32, u32)>,
    scan_qr: bool,
  ) -> Result<u64, String> {
    let still = self.cameras.still.borrow().clone();
    let (camera, request, producer) = match still {
      Some(_) => (None, None, Some(Producer::Still)),
      None => {
        let request = OpenRequest { device, facing, size };
        match ore::camera::start() {
          ore::camera::Start::Ready => (Some(open_device(&request)?), None, None),
          ore::camera::Start::Starting => (None, Some(request), None),
          ore::camera::Start::Failed(e) => return Err(e),
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
        if matches!(session.status, CameraStatus::Pending)
          && !ore::camera::permission_prompts()
          && session.opened_at.elapsed() >= PENDING_DEADLINE
        {
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
    let outcome = match ore::camera::start() {
      ore::camera::Start::Ready => open_device(request),
      ore::camera::Start::Failed(e) => Err(e),
      ore::camera::Start::Starting if session.opened_at.elapsed() >= INIT_DEADLINE => {
        log::warn!("[camera] subsystem did not start within {}s; failing the open", INIT_DEADLINE.as_secs());
        Err(format!("camera subsystem did not start within {}s", INIT_DEADLINE.as_secs()))
      }
      ore::camera::Start::Starting => return,
    };
    session.request = None;
    match outcome {
      Ok(camera) => {
        session.camera = Some(camera);
        session.opened_at = std::time::Instant::now();
      }
      Err(e) => session.status = CameraStatus::Failed(e),
    }
  }

  fn pump_permission(session: &mut Session) {
    let Some(camera) = &session.camera else {
      return;
    };
    match camera.permission() {
      ore::camera::PermissionState::Approved => session.approved = true,
      ore::camera::PermissionState::Denied => {
        log::warn!("[camera] permission denied");
        session.stop();
        session.status = CameraStatus::Denied;
      }
      ore::camera::PermissionState::Pending => {}
    }
  }

  /// Hand the device to the camera thread. A thread that cannot start fails
  /// the session; the device is released with it.
  fn start_producer(session: &mut Session) {
    let Some(camera) = session.camera.take() else {
      return;
    };
    match producer::spawn(camera, session.tap.clone()) {
      Ok(handle) => {
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
