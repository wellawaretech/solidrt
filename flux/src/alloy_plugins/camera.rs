//! JS bindings for camera capture: a thin marshaling layer over alloy::camera.
//!
//! `camera.open()` returns a promise that settles from `tick`, the per-frame
//! hook driven by the FrameRendered handler (like raf::flush): it pumps alloy's
//! camera sessions (permission transitions + frame uploads) and resolves or
//! rejects any open() promises whose session left Pending. The open is work
//! in flight until the device answers, and the device answers to polling
//! only, so a task of the engine pumps the one session between frames until
//! it leaves Pending, then ends the hold and latches the frame that settles
//! the promise (the glyph worker's pattern, font.rs `request_glyphs`): a
//! stepped host runs no frame while work is in flight, so a hold the frame
//! itself had to end would hang its settle. Opening is the permission
//! request; there is no separate permission API (SDL semantics).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use alloy::camera::{CameraFacing, CameraStatus};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object, Persistent, TypedArray};

use crate::plugins::marshal::{bytes_of, OptArg};

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

/// How often a waiter on an open polls its session between frames, ms: a
/// frame period, so the answer lands about when a client's own tick would
/// have found it, while the poll stays cheap (two non-blocking SDL reads).
const OPEN_POLL_MS: u64 = 16;

struct PendingOpen {
  session: u64,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

struct Inner {
  gui: Rc<super::Gui>,
  pending: RefCell<Vec<PendingOpen>>,
  /// The sessions this engine opened and has not closed: what its end
  /// releases (the device and the session's texture).
  sessions: RefCell<HashSet<u64>>,
  /// Per-session JS barcode callback (sessions opened with scan).
  barcode_handlers: RefCell<HashMap<u64, Persistent<Function<'static>>>>,
}

impl Drop for Inner {
  // Engine teardown: the app's handles died with it, so nothing else will
  // ever close its sessions.
  fn drop(&mut self) {
    for session in self.sessions.get_mut().drain() {
      self.gui.alloy.close_camera(session);
    }
  }
}

#[derive(Clone, JsLifetime)]
struct CameraPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the camera plugin state in userdata. Runs at engine init (before any
/// module import) so `CameraModule::evaluate` and the per-frame `tick` can read
/// it. The `flux:camera` module surface is registered separately via
/// `module`.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx
    .store_userdata(CameraPluginState(Rc::new(Inner {
      gui: super::gui(ctx),
      pending: RefCell::new(Vec::new()),
      sessions: RefCell::new(HashSet::new()),
      barcode_handlers: RefCell::new(HashMap::new()),
    })))
    .expect("store camera state");
}

/// The `flux:camera` module. `open` resolves to a bound session object
/// (`{ texture, width, height, onBarcode, close }`) so the handle never leaves
/// Rust; `close`/`setBarcodeCallback` are methods on it rather than module
/// exports taking a raw handle.
pub struct CameraModule;

impl ModuleDef for CameraModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("listCameras")?;
    decl.declare("open")?;
    decl.declare("scanImage")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("listCameras", Function::new(ctx.clone(), list_impl)?)?;
    exports.export("open", Function::new(ctx.clone(), open_impl)?)?;
    exports.export("scanImage", Function::new(ctx.clone(), scan_image_impl)?)?;
    Ok(())
  }
}

fn facing_str(facing: CameraFacing) -> &'static str {
  match facing {
    CameraFacing::Front => "front",
    CameraFacing::Back => "back",
    CameraFacing::Unknown => "unknown",
  }
}

fn list_impl(ctx: Ctx<'_>) -> rquickjs::Result<Array<'_>> {
  let arr = Array::new(ctx.clone())?;
  for (i, cam) in alloy::camera::list_cameras().iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("id", cam.id)?;
    obj.set("name", cam.name.as_str())?;
    obj.set("facing", facing_str(cam.facing))?;
    arr.set(i, obj)?;
  }
  Ok(arr)
}

fn open_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  let mut device: Option<u32> = None;
  let mut facing: Option<CameraFacing> = None;
  let mut width: Option<u32> = None;
  let mut height: Option<u32> = None;
  let mut scan_qr = false;
  if let Some(opts) = options.0 {
    device = opts.get("camera")?;
    let f: Option<String> = opts.get("facing")?;
    facing = match f.as_deref() {
      Some("front") => Some(CameraFacing::Front),
      Some("back") => Some(CameraFacing::Back),
      Some(other) => return Err(throw_str(&ctx, &format!("openCamera: unknown facing '{other}'"))),
      None => None,
    };
    width = opts.get("width")?;
    height = opts.get("height")?;
    let scan: Option<Vec<String>> = opts.get("scan")?;
    for format in scan.unwrap_or_default() {
      match format.as_str() {
        "qr" => scan_qr = true,
        other => return Err(throw_str(&ctx, &format!("openCamera: unsupported scan format '{other}'"))),
      }
    }
  }
  let size = match (width, height) {
    (Some(w), Some(h)) => Some((w, h)),
    _ => None,
  };

  let state = ctx.userdata::<CameraPluginState>().expect("camera state");
  // Option validation above throws (caller bug); a device that cannot be
  // opened rejects the promise instead - the documented contract, and the
  // failure can be environmental (a stale hotplug entry whose /dev node is
  // gone: SDL still lists unplugged Linux cameras, see AlloyEvent doc). A
  // sync throw here unwinds through the caller's reactive computation, which
  // no .catch can intercept (observed as REACTIVITY_HALTED in the player).
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  match state.0.gui.alloy.open_camera(device, facing, size, scan_qr) {
    Ok(session) => {
      state.0.sessions.borrow_mut().insert(session);
      state.0.pending.borrow_mut().push(PendingOpen {
        session,
        resolve: Persistent::save(&ctx, resolve),
        reject: Persistent::save(&ctx, reject),
      });
      // The hold ends when the device answers, polled on the task's own
      // cadence (a client's ticks pump the same session and may get there
      // first); the latch and the wake bring the frame whose `tick` settles
      // the promise. A session closed meanwhile answers None and ends the
      // hold the same way.
      let hold = crate::pending::PendingOps::of(&ctx).in_flight("camera open");
      let latch = state.0.gui.platform.frame_request_handle();
      let wake = state.0.gui.alloy.frame_wake();
      let gui = state.0.gui.clone();
      ctx.spawn(async move {
        while matches!(gui.alloy.pump_camera(session), Some(CameraStatus::Pending)) {
          tokio::time::sleep(std::time::Duration::from_millis(OPEN_POLL_MS)).await;
        }
        drop(hold);
        latch.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(wake) = &wake {
          wake();
        }
      });
    }
    Err(e) => {
      let error = Exception::from_message(ctx.clone(), &format!("openCamera: {e}"))?;
      reject.call::<_, ()>((error,))?;
    }
  }
  Ok(promise)
}

fn close_impl(ctx: Ctx<'_>, session: u64) {
  let state = ctx.userdata::<CameraPluginState>().expect("camera state");
  state.0.gui.alloy.close_camera(session);
  state.0.sessions.borrow_mut().remove(&session);
  state.0.barcode_handlers.borrow_mut().remove(&session);
}

/// Register (or replace) the JS callback receiving decoded barcodes for a
/// session opened with a scan option. The callback's lifetime unifies with the
/// stored Persistent because this is a named generic fn, not a closure.
fn set_barcode_impl<'js>(ctx: Ctx<'js>, session: u64, callback: Function<'js>) {
  let state = ctx.userdata::<CameraPluginState>().expect("camera state");
  state.0.barcode_handlers.borrow_mut().insert(session, Persistent::save(&ctx, callback));
}

/// One-shot scan of an RGBA8 pixel buffer (e.g. decodeImage output); returns
/// every decoded QR code as { data, format: "qr" }.
fn scan_image_impl<'js>(
  ctx: Ctx<'js>,
  data: TypedArray<'js, u8>,
  width: u32,
  height: u32,
) -> rquickjs::Result<Array<'js>> {
  let pixels = bytes_of(&ctx, &data, "scanImage")?;
  let expected = (width as usize) * (height as usize) * 4;
  if pixels.len() != expected {
    return Err(throw_str(&ctx, &format!("scanImage: expected {expected} RGBA8 bytes, got {}", pixels.len())));
  }

  let arr = Array::new(ctx.clone())?;
  for (i, content) in alloy::barcode::scan_rgba(pixels, width, height).into_iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("data", content)?;
    obj.set("format", "qr")?;
    arr.set(i, obj)?;
  }
  Ok(arr)
}

/// Reject an open() promise with an Error carrying `msg`.
fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, msg: &str) {
  let (Ok(func), Ok(error)) = (reject.restore(ctx), Exception::from_message(ctx.clone(), msg)) else {
    return;
  };
  if let Err(e) = func.call::<_, ()>((error,)) {
    log::warn!("[camera] reject call failed: {e}");
  }
}

/// Per-frame hook (see `frame::advance`). Returns true when a camera
/// uploaded a new frame into its texture, so the caller can request a redraw.
pub(crate) fn tick(ctx: &Ctx<'_>) -> bool {
  let Some(state) = ctx.userdata::<CameraPluginState>() else {
    return false;
  };
  let uploaded = state.0.gui.alloy.pump_cameras();
  dispatch_barcodes(ctx, &state);

  if state.0.pending.borrow().is_empty() {
    return uploaded;
  }
  let pending = std::mem::take(&mut *state.0.pending.borrow_mut());
  for entry in pending {
    match state.0.gui.alloy.camera_status(entry.session) {
      Some(CameraStatus::Pending) => state.0.pending.borrow_mut().push(entry),
      Some(CameraStatus::Ready { texture_id, width, height }) => {
        let session = entry.session;
        let settle = || -> rquickjs::Result<()> {
          let obj = Object::new(ctx.clone())?;
          obj.set("texture", texture_id)?;
          obj.set("width", width)?;
          obj.set("height", height)?;
          // close()/onBarcode() are bound to this session so the raw handle
          // never crosses into JS; they reuse the same helpers the global API did.
          let close_fn = Function::new(ctx.clone(), move |ctx: Ctx<'_>| close_impl(ctx, session))?;
          obj.set("close", close_fn)?;
          let on_barcode_fn = Function::new(ctx.clone(), move |callback: Function<'_>| {
            // Derive the Ctx from the callback so their lifetimes unify.
            let ctx = callback.ctx().clone();
            set_barcode_impl(ctx, session, callback);
          })?;
          obj.set("onBarcode", on_barcode_fn)?;
          entry.resolve.restore(ctx)?.call::<_, ()>((obj,))
        };
        if let Err(e) = settle() {
          log::warn!("[camera] resolve call failed: {e}");
        }
      }
      Some(CameraStatus::Denied) => reject_with(ctx, entry.reject, "camera permission denied"),
      Some(CameraStatus::Failed(message)) => reject_with(ctx, entry.reject, &message),
      None => reject_with(ctx, entry.reject, "camera closed"),
    }
  }
  uploaded
}

/// Forward decoded barcodes to their session's JS callback as
/// `{ data, format: "qr" }`, dropping handlers of closed sessions.
fn dispatch_barcodes(ctx: &Ctx<'_>, state: &CameraPluginState) {
  if state.0.barcode_handlers.borrow().is_empty() {
    return;
  }
  let handlers: Vec<(u64, Persistent<Function<'static>>)> =
    state.0.barcode_handlers.borrow().iter().map(|(sid, f)| (*sid, f.clone())).collect();
  for (session, handler) in handlers {
    if state.0.gui.alloy.camera_status(session).is_none() {
      state.0.barcode_handlers.borrow_mut().remove(&session);
      continue;
    }
    for data in state.0.gui.alloy.take_camera_barcodes(session) {
      let call = || -> rquickjs::Result<()> {
        let obj = Object::new(ctx.clone())?;
        obj.set("data", data.as_str())?;
        obj.set("format", "qr")?;
        handler.clone().restore(ctx)?.call::<_, ()>((obj,))
      };
      if let Err(e) = call() {
        log::warn!("[camera] barcode callback failed: {e}");
      }
    }
  }
}
