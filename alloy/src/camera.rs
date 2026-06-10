//! Camera capture via the SDL camera subsystem.
//!
//! A session opens an SDL camera at an uncompressed native format near the
//! requested size (frames are converted to RGBA32 in the pump) and exposes
//! them through a registry texture (see `Context::pump_cameras`), so a camera
//! view is just a texture draw for whatever sits on top. Opening triggers the
//! OS permission prompt; sessions start `Pending` and become `Ready` (texture
//! created at the delivered format) or `Denied`. The pump is driven once per
//! frame from the UI thread; `SDL_AcquireCameraFrame` is non-blocking, so no
//! camera thread is needed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

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
  /// Waiting for the OS permission prompt.
  Pending,
  /// Streaming into `texture_id` at the delivered size.
  Ready { texture_id: u64, width: u32, height: u32 },
  Denied,
}

struct Session {
  camera: *mut SDL_Camera,
  status: CameraStatus,
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

/// The uncompressed native spec closest to the requested size (ties broken by
/// framerate closest to 30), or None when the camera only offers compressed
/// formats (e.g. MJPG-only at every size).
fn native_spec(id: u32, width: u32, height: u32) -> Option<SDL_CameraSpec> {
  let target = (width as i64) * (height as i64);
  sdl_utils::camera_supported_formats(id)
    .into_iter()
    .filter(|s| s.format != SDL_PIXELFORMAT_MJPG)
    .min_by_key(|s| {
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
    scan_qr: bool,
  ) -> Result<u64, String> {
    ensure_init()?;
    let id = match device {
      Some(d) => d,
      None => {
        let cams = list_cameras();
        // Default to the back camera: viewfinders and scanning overwhelmingly
        // want it on phones, and desktop webcams (facing Unknown) fall through
        // to "first camera" either way.
        let want = facing.unwrap_or(CameraFacing::Back);
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
    let (width, height) = size.unwrap_or((640, 480));
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
        approved: false,
        texture_id: self.textures.allocate_id(),
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
        CameraStatus::Pending => {
          if !session.approved {
            Self::pump_permission(session);
          }
          if session.approved {
            self.pump_frame(session);
          }
        }
        CameraStatus::Ready { .. } => self.pump_frame(session),
        CameraStatus::Denied => {}
      }
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
  /// (device rotated between portrait and landscape).
  fn pump_frame(&self, session: &mut Session) {
    let frame = sdl_utils::camera_acquire_frame(session.camera);
    if frame.is_null() {
      return;
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
        return;
      }
    }
    let pixels: &[u8] = if rgba_src && rotation == 0 && pitch == row_bytes {
      unsafe { std::slice::from_raw_parts(surface.pixels as *const u8, row_bytes * frame_h as usize) }
    } else if !rgba_src && rotation == 0 {
      &session.convert
    } else {
      let (src, src_pitch) = if rgba_src { (surface.pixels as *const u8, pitch) } else { (session.convert.as_ptr(), row_bytes) };
      upright_into(src, src_pitch, frame_w as usize, frame_h as usize, rotation, &mut session.scratch);
      &session.scratch
    };

    let recreate = match session.status {
      CameraStatus::Ready { width: w, height: h, .. } => w != width || h != height,
      _ => true,
    };
    if recreate {
      self.create_texture_at(session.texture_id, width, height, pixels);
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
