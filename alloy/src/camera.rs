//! Camera capture via the SDL camera subsystem.
//!
//! A session opens an SDL camera requesting RGBA32 frames and exposes them
//! through a registry texture (see `Context::pump_cameras`), so a camera view
//! is just a texture draw for whatever sits on top. Opening triggers the OS
//! permission prompt; sessions start `Pending` and become `Ready` (texture
//! created at the delivered format) or `Denied`. The pump is driven once per
//! frame from the UI thread; `SDL_AcquireCameraFrame` is non-blocking, so no
//! camera thread is needed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use sdl3::sys::camera::{SDL_Camera, SDL_CameraPosition, SDL_CameraSpec};
use sdl3::sys::pixels::{SDL_COLORSPACE_SRGB, SDL_PIXELFORMAT_RGBA32};

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
  /// Waiting for the OS permission prompt.
  Pending,
  /// Streaming into `texture_id` at the delivered size.
  Ready { texture_id: u64, width: u32, height: u32 },
  Denied,
}

struct Session {
  camera: *mut SDL_Camera,
  status: CameraStatus,
  /// Row-repack buffer for frames whose pitch exceeds width * 4.
  scratch: Vec<u8>,
}

#[derive(Default)]
pub struct CameraRegistry {
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
}

/// Lazy one-time SDL camera subsystem init (first list/open call).
fn ensure_init() -> Result<(), String> {
  static INIT: OnceLock<bool> = OnceLock::new();
  let ok = *INIT.get_or_init(sdl_utils::camera_subsystem_init);
  if ok {
    Ok(())
  } else {
    Err(format!("camera subsystem init failed: {}", sdl_utils::sdl_error()))
  }
}

fn facing_of(position: SDL_CameraPosition) -> CameraFacing {
  match position {
    SDL_CameraPosition::FRONT_FACING => CameraFacing::Front,
    SDL_CameraPosition::BACK_FACING => CameraFacing::Back,
    _ => CameraFacing::Unknown,
  }
}

pub fn list_cameras() -> Vec<CameraInfo> {
  if ensure_init().is_err() {
    log::warn!("[camera] {}", sdl_utils::sdl_error());
    return Vec::new();
  }
  sdl_utils::camera_ids()
    .into_iter()
    .map(|id| CameraInfo { id, name: sdl_utils::camera_name(id), facing: facing_of(sdl_utils::camera_position(id)) })
    .collect()
}

impl crate::context::Context {
  /// Open a camera session. `device` picks an explicit camera id, otherwise
  /// the first camera matching `facing` (or simply the first one). Returns the
  /// session id; the session is `Pending` until the OS permission resolves.
  pub fn open_camera(
    &self,
    device: Option<u32>,
    facing: Option<CameraFacing>,
    size: Option<(u32, u32)>,
  ) -> Result<u64, String> {
    ensure_init()?;
    let id = match device {
      Some(d) => d,
      None => {
        let cams = list_cameras();
        let preferred = facing.and_then(|f| cams.iter().find(|c| c.facing == f));
        preferred.or_else(|| cams.first()).map(|c| c.id).ok_or_else(|| "no cameras available".to_string())?
      }
    };

    // Ask SDL for RGBA32 directly; it converts from the native format when
    // needed, and the delivered spec is re-read on approval either way.
    let (width, height) = size.unwrap_or((640, 480));
    let spec = SDL_CameraSpec {
      format: SDL_PIXELFORMAT_RGBA32,
      colorspace: SDL_COLORSPACE_SRGB,
      width: width as i32,
      height: height as i32,
      framerate_numerator: 30,
      framerate_denominator: 1,
    };
    let camera = sdl_utils::camera_open(id, &spec);
    if camera.is_null() {
      return Err(format!("failed to open camera {id}: {}", sdl_utils::sdl_error()));
    }

    let sid = {
      let mut next = self.cameras.next_id.borrow_mut();
      *next += 1;
      *next
    };
    self
      .cameras
      .sessions
      .borrow_mut()
      .insert(sid, Session { camera, status: CameraStatus::Pending, scratch: Vec::new() });
    Ok(sid)
  }

  pub fn camera_status(&self, sid: u64) -> Option<CameraStatus> {
    self.cameras.sessions.borrow().get(&sid).map(|s| s.status.clone())
  }

  /// Close the session and release the device. The session's texture stays in
  /// the registry showing its last frame.
  pub fn close_camera(&self, sid: u64) {
    if let Some(session) = self.cameras.sessions.borrow_mut().remove(&sid) {
      sdl_utils::camera_close(session.camera);
    }
  }

  /// Release every open camera. Called between engine runs so a reloaded app
  /// never inherits (or leaks) a live capture device.
  pub fn close_all_cameras(&self) {
    for (_, session) in self.cameras.sessions.borrow_mut().drain() {
      sdl_utils::camera_close(session.camera);
    }
  }

  /// Advance all sessions: resolve pending permission prompts and upload the
  /// latest frame of each ready session into its texture. Run once per frame
  /// on the UI thread; does nothing when no sessions are open.
  pub fn pump_cameras(&self) {
    let mut sessions = self.cameras.sessions.borrow_mut();
    for session in sessions.values_mut() {
      match session.status {
        CameraStatus::Pending => self.pump_pending(session),
        CameraStatus::Ready { texture_id, width, height } => self.pump_frame(session, texture_id, width, height),
        CameraStatus::Denied => {}
      }
    }
  }

  fn pump_pending(&self, session: &mut Session) {
    use sdl3::sys::camera::SDL_CameraPermissionState as P;
    match sdl_utils::camera_permission(session.camera) {
      P::APPROVED => {
        let Some(spec) = sdl_utils::camera_format(session.camera) else {
          log::error!("[camera] approved but no format: {}", sdl_utils::sdl_error());
          sdl_utils::camera_close(session.camera);
          session.status = CameraStatus::Denied;
          return;
        };
        let (width, height) = (spec.width as u32, spec.height as u32);
        // Opaque black placeholder until the first frame arrives.
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        for px in pixels.chunks_exact_mut(4) {
          px[3] = 255;
        }
        let texture_id = self.create_texture_from_pixels(width, height, &pixels);
        log::info!("[camera] ready: {width}x{height} -> texture {texture_id}");
        session.status = CameraStatus::Ready { texture_id, width, height };
      }
      P::DENIED => {
        log::warn!("[camera] permission denied");
        sdl_utils::camera_close(session.camera);
        session.status = CameraStatus::Denied;
      }
      _ => {}
    }
  }

  fn pump_frame(&self, session: &mut Session, texture_id: u64, width: u32, height: u32) {
    let frame = sdl_utils::camera_acquire_frame(session.camera);
    if frame.is_null() {
      return;
    }
    let surface = unsafe { &*frame };
    let row_bytes = (width as usize) * 4;
    if surface.format != SDL_PIXELFORMAT_RGBA32 || surface.w as u32 != width || surface.h as u32 != height {
      log::warn!(
        "[camera] unexpected frame {}x{} format {:#x}, expected {width}x{height} RGBA32",
        surface.w,
        surface.h,
        surface.format.0
      );
      sdl_utils::camera_release_frame(session.camera, frame);
      return;
    }

    let pitch = surface.pitch as usize;
    if pitch == row_bytes {
      let pixels = unsafe { std::slice::from_raw_parts(surface.pixels as *const u8, row_bytes * height as usize) };
      let _ = self.update_texture(texture_id, pixels, 0);
    } else {
      session.scratch.resize(row_bytes * height as usize, 0);
      for row in 0..height as usize {
        let src = unsafe { std::slice::from_raw_parts((surface.pixels as *const u8).add(row * pitch), row_bytes) };
        session.scratch[row * row_bytes..(row + 1) * row_bytes].copy_from_slice(src);
      }
      let _ = self.update_texture(texture_id, &session.scratch, 0);
    }
    sdl_utils::camera_release_frame(session.camera, frame);
  }
}