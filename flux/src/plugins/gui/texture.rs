use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Ctx, Exception, Function, JsLifetime, Object, Persistent, TypedArray};

use super::AlloyContext;
use alloy::rendertree::PlatformContext;
use alloy::CaptureInfo;

// Per-engine texture bookkeeping, held in context userdata so engine teardown
// (which clears userdata while the runtime is still alive) destroys the GPU
// textures this engine created.
#[derive(Clone, JsLifetime)]
struct TextureState(#[qjs(skip_trace)] Rc<TextureInner>);

struct TextureInner {
  atx: AlloyContext,
  // Held so the module's evaluate (which only gets a Ctx) can request frames
  // after an upload / shader-param change, the way the global closures captured
  // it before.
  platform: Arc<PlatformContext>,
  // Every texture id this engine created (immutable, mutable, shader, and
  // captureSnapshot output). The alloy texture registry outlives the engine, so
  // without this a reload leaks the previous app's textures - the app rarely
  // calls destroyTexture itself.
  created: RefCell<HashSet<u64>>,
  // captureSnapshot is async: alloy services the request on a later paint pass
  // and invokes our completion callback (during `deliver_captures`), which moves
  // the outcome plus the promise sides here. `tick` then drains this and settles
  // each promise with the live `Ctx` it holds (the callback has none). Two hops
  // because a JS promise can only be touched from the JS thread with a `Ctx`.
  capture_settle: RefCell<Vec<CaptureSettle>>,
}

struct CaptureSettle {
  result: Result<CaptureInfo, String>,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

impl Drop for TextureInner {
  fn drop(&mut self) {
    for id in self.created.borrow_mut().drain() {
      self.atx.destroy_texture(id);
    }
  }
}

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  ctx.throw(rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

// Flatten a JS { name: number } object into the (name, f32) pairs alloy matches
// against the shader's uniforms by name. Non-numeric values are skipped.
fn collect_params(obj: &Object<'_>) -> Vec<(String, f32)> {
  obj.props::<String, f64>().filter_map(|r| r.ok()).map(|(k, v)| (k, v as f32)).collect()
}

// Flatten a JS { samplerName: textureId } object into (name, id) pairs alloy
// binds to the shader's sampler2D uniforms. Non-numeric values are skipped.
fn collect_textures(obj: &Object<'_>) -> Vec<(String, u64)> {
  obj.props::<String, u64>().filter_map(|r| r.ok()).collect()
}

/// Store the texture plugin state (alloy context, platform, and the created-id
/// set for reload cleanup) in userdata, before any module import. The
/// `flux:gpu` surface is registered separately via `module_override`.
pub fn store_state(ctx: &Ctx<'_>, atx: AlloyContext, platform: Arc<PlatformContext>) {
  ctx
    .store_userdata(TextureState(Rc::new(TextureInner {
      atx,
      platform,
      created: RefCell::new(HashSet::new()),
      capture_settle: RefCell::new(Vec::new()),
    })))
    .expect("store texture state");
}

/// The `flux:gpu` module: texture creation/upload/destruction and fragment
/// shaders. Texture ids are the public token (used as `<texture src>` and shader
/// sampler inputs), so there is no handle to hide here.
pub struct GpuModule;

impl ModuleDef for GpuModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createTexture")?;
    decl.declare("createMutableTexture")?;
    decl.declare("uploadTexture")?;
    decl.declare("destroyTexture")?;
    decl.declare("createShader")?;
    decl.declare("setShaderParams")?;
    decl.declare("captureSnapshot")?;
    decl.declare("readTexture")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let state = ctx.userdata::<TextureState>().expect("texture state userdata");
    let atx = state.0.atx.clone();
    let platform = state.0.platform.clone();

    let create_atx = atx.clone();
    let create_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, data: TypedArray<'_, u8>, width: u32, height: u32| -> rquickjs::Result<u64> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createTexture: detached buffer"))?;
        let expected = (width as usize) * (height as usize) * 4;
        if raw.len != expected {
          return Err(throw_str(&ctx, &format!("createTexture: expected {expected} RGBA8 bytes, got {}", raw.len)));
        }
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        let id = create_atx.create_texture_from_pixels(width, height, pixels);
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createTexture");

    // A mutable texture is created exactly like an immutable one; "mutable" only
    // signals intent to update it later via uploadTexture. The seed buffer may be
    // larger than one frame (uploadTexture takes an offset), so only require that
    // the first frame fits.
    let mutable_atx = atx.clone();
    let create_mutable_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, data: TypedArray<'_, u8>, width: u32, height: u32| -> rquickjs::Result<u64> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createMutableTexture: detached buffer"))?;
        let frame_size = (width as usize) * (height as usize) * 4;
        if raw.len < frame_size {
          return Err(throw_str(
            &ctx,
            &format!("createMutableTexture: need at least {frame_size} RGBA8 bytes, got {}", raw.len),
          ));
        }
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), frame_size) };
        let id = mutable_atx.create_texture_from_pixels(width, height, pixels);
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createMutableTexture");

    // The caller passes the pixel buffer on every upload (it already holds it),
    // so nothing is pinned on the Rust side. `data` may hold multiple frames;
    // `offset` selects the one to upload. Reading the buffer is zero-copy.
    let upload_atx = atx.clone();
    let upload_platform = platform.clone();
    let upload_texture = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>, id: u64, data: TypedArray<'_, u8>, offset: Opt<usize>| -> rquickjs::Result<()> {
        let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "uploadTexture: detached buffer"))?;
        let pixels = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
        upload_atx
          .update_texture(id, pixels, offset.0.unwrap_or(0))
          .map_err(|e| throw_str(&ctx, &format!("uploadTexture: {e}")))?;
        // New texture content changes the screen without any tree mutation.
        upload_platform.request_frame();
        Ok(())
      },
    )
    .expect("create uploadTexture");

    let create_shader_atx = atx.clone();
    let create_shader = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'_>,
            fragment_src: String,
            width: u32,
            height: u32,
            params: Option<Object<'_>>,
            textures: Option<Object<'_>>|
            -> rquickjs::Result<u64> {
        let params = params.as_ref().map(collect_params).unwrap_or_default();
        let textures = textures.as_ref().map(collect_textures).unwrap_or_default();
        let id = create_shader_atx
          .create_shader_texture(width, height, &fragment_src, &params, &textures)
          .map_err(|e| throw_str(&ctx, &format!("createShader: {e}")))?;
        let state = ctx.userdata::<TextureState>().expect("texture state userdata");
        state.0.created.borrow_mut().insert(id);
        Ok(id)
      },
    )
    .expect("create createShader");

    let set_params_atx = atx.clone();
    let set_params_platform = platform.clone();
    let set_shader_params =
      Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64, params: Object<'_>| -> rquickjs::Result<()> {
        let params = collect_params(&params);
        set_params_atx
          .update_shader_params(id, &params)
          .map_err(|e| throw_str(&ctx, &format!("setShaderParams: {e}")))?;
        // New shader output changes the screen without any tree mutation.
        set_params_platform.request_frame();
        Ok(())
      })
      .expect("create setShaderParams");

    let destroy_atx = atx.clone();
    let destroy_texture = Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u64| {
      let state = ctx.userdata::<TextureState>().expect("texture state userdata");
      state.0.created.borrow_mut().remove(&id);
      destroy_atx.destroy_texture(id);
    })
    .expect("create destroyTexture");

    exports.export("createTexture", create_texture)?;
    exports.export("createMutableTexture", create_mutable_texture)?;
    exports.export("uploadTexture", upload_texture)?;
    exports.export("destroyTexture", destroy_texture)?;
    exports.export("createShader", create_shader)?;
    exports.export("setShaderParams", set_shader_params)?;
    // Named generic fns, not closures: `captureSnapshot` returns a Promise and
    // `readTexture` an Object, whose 'js lifetime must unify with the Ctx arg -
    // a closure gives them independent invariant lifetimes and will not compile
    // (same reason camera::open is a named fn). They read state from userdata.
    exports.export("captureSnapshot", Function::new(ctx.clone(), capture_snapshot_impl)?)?;
    exports.export("readTexture", Function::new(ctx.clone(), read_texture_impl)?)?;
    Ok(())
  }
}

/// Queue a node capture and return a promise settled from `tick` once alloy
/// renders it on a later paint pass. The completion callback (run on the UI
/// thread with no `Ctx`) only stashes the outcome and the promise sides into
/// `capture_settle`; `tick` does the JS-facing settle (see CaptureSettle).
fn capture_snapshot_impl<'js>(ctx: Ctx<'js>, node_id: u64) -> rquickjs::Result<Promise<'js>> {
  let state = ctx.userdata::<TextureState>().expect("texture state userdata");
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let resolve = Persistent::save(&ctx, resolve);
  let reject = Persistent::save(&ctx, reject);
  // The callback holds an Rc to our state so it can enqueue the outcome. Alloy
  // owns the callback until the capture is serviced, keeping the state alive
  // while the request is in flight (no cycle: alloy is not owned by the state).
  let inner = state.0.clone();
  state.0.atx.request_capture(
    node_id,
    Box::new(move |result| {
      inner.capture_settle.borrow_mut().push(CaptureSettle { result, resolve, reject });
    }),
  );
  // The capture is serviced during a paint; make sure one happens.
  state.0.platform.request_frame();
  Ok(promise)
}

/// Read back any registered texture's current RGBA8 pixels (tightly packed,
/// top-to-bottom) as `{ width, height, data }`. Synchronous: the texture was
/// already rendered on this thread's GL context at creation time, so there is
/// nothing to wait for.
fn read_texture_impl<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Object<'js>> {
  let state = ctx.userdata::<TextureState>().expect("texture state userdata");
  let (width, height, pixels) =
    state.0.atx.read_texture_by_id(id).map_err(|e| throw_str(&ctx, &format!("readTexture: {e}")))?;
  let obj = Object::new(ctx.clone())?;
  obj.set("width", width)?;
  obj.set("height", height)?;
  obj.set("data", TypedArray::new(ctx.clone(), pixels)?)?;
  Ok(obj)
}

/// Reject a captureSnapshot promise with an Error carrying `msg`.
fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, msg: &str) {
  let (Ok(func), Ok(error)) = (reject.restore(ctx), Exception::from_message(ctx.clone(), msg)) else {
    return;
  };
  if let Err(e) = func.call::<_, ()>((error,)) {
    log::warn!("[gpu] capture reject call failed: {e}");
  }
}

/// Per-frame hook, called alongside `camera::tick` / `raf::flush`. Drains the
/// capture outcomes the completion callbacks enqueued during the last paint and
/// settles each promise, tracking the new texture id for reload cleanup.
pub fn tick(ctx: &Ctx<'_>) {
  let Some(state) = ctx.userdata::<TextureState>() else {
    return;
  };
  let settles = std::mem::take(&mut *state.0.capture_settle.borrow_mut());
  for CaptureSettle { result, resolve, reject } in settles {
    match result {
      Ok(CaptureInfo { texture_id, width, height }) => {
        // Same reload-cleanup bookkeeping the create* functions do.
        state.0.created.borrow_mut().insert(texture_id);
        let settle = || -> rquickjs::Result<()> {
          let obj = Object::new(ctx.clone())?;
          obj.set("id", texture_id)?;
          obj.set("width", width)?;
          obj.set("height", height)?;
          resolve.restore(ctx)?.call::<_, ()>((obj,))
        };
        if let Err(e) = settle() {
          log::warn!("[gpu] capture resolve failed: {e}");
        }
      }
      Err(error) => reject_with(ctx, reject, &error),
    }
  }
}
