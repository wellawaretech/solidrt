//! JS bindings for camera capture: a thin marshaling layer over alloy::camera.
//!
//! `camera.open()` returns a promise that settles from `tick`, the per-frame
//! hook driven by the FrameRendered handler (like raf::flush): it pumps alloy's
//! camera sessions (permission transitions + frame uploads) and resolves or
//! rejects any open() promises whose session left Pending. Opening is the
//! permission request; there is no separate permission API (SDL semantics).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use alloy::camera::{CameraFacing, CameraStatus};
use flux::rquickjs::function::Opt;
use flux::rquickjs::promise::Promise;
use flux::rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object, Persistent, TypedArray};

use crate::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> flux::rquickjs::Error {
  ctx.throw(flux::rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

struct PendingOpen {
  session: u64,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

struct Inner {
  atx: AlloyContext,
  pending: RefCell<Vec<PendingOpen>>,
  /// Per-session JS barcode callback (sessions opened with scan).
  barcode_handlers: RefCell<HashMap<u64, Persistent<Function<'static>>>>,
}

#[derive(Clone, JsLifetime)]
struct CameraPluginState(#[qjs(skip_trace)] Rc<Inner>);

pub fn init(ctx: Ctx<'_>, atx: AlloyContext) {
  ctx
    .store_userdata(CameraPluginState(Rc::new(Inner {
      atx,
      pending: RefCell::new(Vec::new()),
      barcode_handlers: RefCell::new(HashMap::new()),
    })))
    .expect("store camera state");

  let list = Function::new(ctx.clone(), list_impl).expect("create camera.listCameras");
  let open = Function::new(ctx.clone(), open_impl).expect("create camera.open");
  let close = Function::new(ctx.clone(), close_impl).expect("create camera.close");
  let set_barcode = Function::new(ctx.clone(), set_barcode_impl).expect("create camera.setBarcodeCallback");
  let scan_image = Function::new(ctx.clone(), scan_image_impl).expect("create camera.scanImage");

  let camera = Object::new(ctx.clone()).expect("create camera object");
  camera.set("listCameras", list).expect("set camera.listCameras");
  camera.set("open", open).expect("set camera.open");
  camera.set("close", close).expect("set camera.close");
  camera.set("setBarcodeCallback", set_barcode).expect("set camera.setBarcodeCallback");
  camera.set("scanImage", scan_image).expect("set camera.scanImage");
  ctx.globals().set("camera", camera).expect("set camera global");
}

fn facing_str(facing: CameraFacing) -> &'static str {
  match facing {
    CameraFacing::Front => "front",
    CameraFacing::Back => "back",
    CameraFacing::Unknown => "unknown",
  }
}

fn list_impl(ctx: Ctx<'_>) -> flux::rquickjs::Result<Array<'_>> {
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

fn open_impl<'js>(ctx: Ctx<'js>, options: Opt<Object<'js>>) -> flux::rquickjs::Result<Promise<'js>> {
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
  let session = state
    .0
    .atx
    .open_camera(device, facing, size, scan_qr)
    .map_err(|e| throw_str(&ctx, &format!("openCamera: {e}")))?;

  let (promise, resolve, reject) = Promise::new(&ctx)?;
  state.0.pending.borrow_mut().push(PendingOpen {
    session,
    resolve: Persistent::save(&ctx, resolve),
    reject: Persistent::save(&ctx, reject),
  });
  Ok(promise)
}

fn close_impl(ctx: Ctx<'_>, session: u64) {
  let state = ctx.userdata::<CameraPluginState>().expect("camera state");
  state.0.atx.close_camera(session);
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
) -> flux::rquickjs::Result<Array<'js>> {
  let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "scanImage: detached buffer"))?;
  let expected = (width as usize) * (height as usize) * 4;
  if raw.len != expected {
    return Err(throw_str(&ctx, &format!("scanImage: expected {expected} RGBA8 bytes, got {}", raw.len)));
  }
  let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };

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

/// Per-frame hook, called from the FrameRendered handler alongside raf::flush.
/// Returns true when a camera uploaded a new frame into its texture, so the
/// caller can request a redraw.
pub fn tick(ctx: &Ctx<'_>) -> bool {
  let Some(state) = ctx.userdata::<CameraPluginState>() else {
    return false;
  };
  let uploaded = state.0.atx.pump_cameras();
  dispatch_barcodes(ctx, &state);

  if state.0.pending.borrow().is_empty() {
    return uploaded;
  }
  let pending = std::mem::take(&mut *state.0.pending.borrow_mut());
  for entry in pending {
    match state.0.atx.camera_status(entry.session) {
      Some(CameraStatus::Pending) => state.0.pending.borrow_mut().push(entry),
      Some(CameraStatus::Ready { texture_id, width, height }) => {
        let settle = || -> flux::rquickjs::Result<()> {
          let obj = Object::new(ctx.clone())?;
          obj.set("handle", entry.session)?;
          obj.set("texture", texture_id)?;
          obj.set("width", width)?;
          obj.set("height", height)?;
          entry.resolve.restore(ctx)?.call::<_, ()>((obj,))
        };
        if let Err(e) = settle() {
          log::warn!("[camera] resolve call failed: {e}");
        }
      }
      Some(CameraStatus::Denied) => reject_with(ctx, entry.reject, "camera permission denied"),
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
    if state.0.atx.camera_status(session).is_none() {
      state.0.barcode_handlers.borrow_mut().remove(&session);
      continue;
    }
    for data in state.0.atx.take_camera_barcodes(session) {
      let call = || -> flux::rquickjs::Result<()> {
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