use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Array, ArrayBuffer, Ctx, Exception, Function, JsLifetime, Object, Persistent, TypedArray, Value};

use super::properties::{decode_params, decode_texture_bindings};
use super::tree::to_prop_value;
use crate::plugins::marshal::{array_buffer_over, OptArg};
use alloy::CaptureInfo;

// Per-engine texture bookkeeping, held in context userdata so engine teardown
// (which clears userdata while the runtime is still alive) destroys the GPU
// textures this engine created.
#[derive(Clone, JsLifetime)]
struct TextureState(#[qjs(skip_trace)] Rc<TextureInner>);

struct TextureInner {
  // The shared host handles: alloy for every resource call and the teardown
  // release in Drop, platform for the frame request after a content write.
  gui: Rc<super::Gui>,
  // Every texture id this engine created (immutable, mutable, and shader).
  // The alloy texture registry outlives the engine, so without this a reload
  // leaks the previous app's textures - the app rarely calls destroyTexture
  // itself.
  created: RefCell<HashSet<u64>>,
  // Same bookkeeping for vertex buffers (their own id space in alloy).
  created_buffers: RefCell<HashSet<u64>>,
  // Open buffer write leases (beginBufferWrite): the minted JS view, kept so
  // end/destroy can detach it before the staging block moves or dies. The
  // block's bytes are pinned by alloy's Context (its WriteLeases), never by
  // QuickJS - the view has no free callback (see array_buffer_over).
  open_buffer_writes: RefCell<HashMap<u64, OpenLease>>,
  // Same bookkeeping for linked programs, render pipelines, and compiled raw
  // stages (each their own id space in alloy).
  created_programs: RefCell<HashSet<u64>>,
  created_pipelines: RefCell<HashSet<u64>>,
  created_stages: RefCell<HashSet<u64>>,
  // captureSnapshot is async: alloy services the request on a later paint pass
  // and invokes our completion callback (during `deliver_captures`), which moves
  // the outcome plus the promise sides here. `tick` then drains this and settles
  // each promise with the live `Ctx` it holds (the callback has none). Two hops
  // because a JS promise can only be touched from the JS thread with a `Ctx`.
  capture_settle: RefCell<Vec<CaptureSettle>>,
}

struct OpenLease {
  view: Persistent<ArrayBuffer<'static>>,
  size: usize,
}

struct CaptureSettle {
  result: Result<CaptureInfo, String>,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

impl Drop for TextureInner {
  fn drop(&mut self) {
    // The window shader first, unconditionally: it is raster-thread state
    // cleared only by an explicit command, and a dying app's declaration is
    // stale by definition. Without this, an app that declared one leaves the
    // next app (or the launcher) rendering through it - the raster thread
    // holds the program by Rc, so even the program destroys below would not
    // stop the pass.
    self.gui.alloy.set_window_shader(None).ok();
    // Then textures before buffers: destroying a pipeline before its buffer
    // is the documented order for destroy_gpu_buffer. Pipelines, programs and
    // stages are order-safe (targets keep their pipeline alive, pipelines
    // their program, programs their own compiled stage copies), released last
    // for symmetry.
    for id in self.created.borrow_mut().drain() {
      self.gui.alloy.destroy_texture(id);
    }
    for id in self.created_buffers.borrow_mut().drain() {
      self.gui.alloy.destroy_gpu_buffer(id);
    }
    for id in self.created_pipelines.borrow_mut().drain() {
      self.gui.alloy.destroy_render_pipeline(id);
    }
    for id in self.created_programs.borrow_mut().drain() {
      self.gui.alloy.destroy_shader_program(id);
    }
    for id in self.created_stages.borrow_mut().drain() {
      self.gui.alloy.destroy_shader_stage(id);
    }
  }
}

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

/// The plugin state every binding reads: stored by `store_state` before any
/// import, so a missing entry is a registration bug, not JS input.
fn state(ctx: &Ctx<'_>) -> Rc<TextureInner> {
  ctx.userdata::<TextureState>().expect("texture state userdata").0.clone()
}

// A JS { name: number | number[] } params object as the (name, value) pairs
// alloy validates against the program's reflected uniform table and dispatches
// by the declared GLSL type. The shape has one decoder, the property
// adapter's (`properties::decode_params`, shared with the `<texture params>`
// and shader props): the object crosses the same PropValue boundary a tree
// write does, so the imperative and the prop path accept and reject
// identically (an undefined entry is skipped, a non-numeric one throws).
// Errors carry the api name. Name-level validation (unknown uniform,
// component count) is alloy's.
fn collect_params(ctx: &Ctx<'_>, obj: &Object<'_>, api: &str) -> rquickjs::Result<Vec<(String, alloy::ParamValue)>> {
  let value = to_prop_value(obj.as_value())?;
  decode_params(&value).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))
}

// A JS { samplerName: textureId | { id, filter?, wrap? } } object as the
// bindings alloy validates against the program's sampler uniforms; the same
// one-decoder rule as collect_params (`properties::decode_texture_bindings`).
fn collect_textures(ctx: &Ctx<'_>, obj: &Object<'_>, api: &str) -> rquickjs::Result<Vec<alloy::TextureBinding>> {
  let value = to_prop_value(obj.as_value())?;
  decode_texture_bindings(&value).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))
}

// Decode the { filter?, wrap?, mipmap?, anisotropy? } sampling options every
// create path accepts ("linear"/"nearest", "clamp"/"repeat", bool, number >= 1;
// defaults linear/clamp/false/1); an unknown value throws at the create call
// site. Resolved against the texture's format: float formats default to
// nearest and refuse linear/mipmap/anisotropy (alloy's parse_for invariant);
// paths without a format option are always rgba8.
fn collect_sampler(
  ctx: &Ctx<'_>,
  opts: &Option<Object<'_>>,
  format: alloy::TextureFormat,
  api: &str,
) -> rquickjs::Result<alloy::SamplerState> {
  let (filter, wrap, mipmap, anisotropy) = match opts {
    Some(o) => (
      o.get::<_, Option<String>>("filter")?,
      o.get::<_, Option<String>>("wrap")?,
      o.get::<_, Option<bool>>("mipmap")?,
      o.get::<_, Option<f64>>("anisotropy")?,
    ),
    None => (None, None, None, None),
  };
  let state = alloy::SamplerState::parse_for(
    format,
    &alloy::SamplerOptions { filter: filter.as_deref(), wrap: wrap.as_deref(), mipmap, anisotropy },
  )
  .map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?;
  // Not invalid (GL accepts it), but almost always a forgotten flag: the
  // level filters through the mip chain, and without one it does next to
  // nothing at the grazing angles it is set for.
  if state.anisotropy > 1 && !state.mipmap {
    log::warn!("[gpu] {api}: anisotropy {} without mipmap: true has little effect - set both", state.anisotropy);
  }
  Ok(state)
}

// Decode the { label? } debug name every create path accepts: free-form, not
// unique, surfaced in the resource inventory and engine log messages.
fn collect_label(opts: &Option<Object<'_>>) -> rquickjs::Result<Option<String>> {
  match opts {
    Some(o) => o.get::<_, Option<String>>("label"),
    None => Ok(None),
  }
}

// Decode the { format? } pixel format the pixel-upload creates accept
// ("rgba8" default | "r8" | "r32f" | "rgba32f"); an unknown value throws at
// the create call site.
fn collect_format(ctx: &Ctx<'_>, opts: &Option<Object<'_>>, api: &str) -> rquickjs::Result<alloy::TextureFormat> {
  let format = match opts {
    Some(o) => o.get::<_, Option<String>>("format")?,
    None => None,
  };
  alloy::TextureFormat::parse(format.as_deref()).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))
}

// The pixel payload of a texture create/upload. The view type must match the
// id's format - byte formats take a Uint8Array, float formats a Float32Array -
// so float data handed as raw bytes (or the reverse) throws at the call site
// instead of uploading a reinterpretation. Held (not just borrowed) so the JS
// buffer stays pinned while the bytes are read.
enum PixelData<'js> {
  Bytes(TypedArray<'js, u8>),
  Floats(TypedArray<'js, f32>),
}

impl<'js> PixelData<'js> {
  fn collect(ctx: &Ctx<'_>, data: Value<'js>, format: alloy::TextureFormat, api: &str) -> rquickjs::Result<Self> {
    if format.is_float() {
      TypedArray::<f32>::from_value(data)
        .map(PixelData::Floats)
        .map_err(|_| throw_str(ctx, &format!("{api}: {} data must be a Float32Array", format.name())))
    } else {
      TypedArray::<u8>::from_value(data)
        .map(PixelData::Bytes)
        .map_err(|_| throw_str(ctx, &format!("{api}: {} data must be a Uint8Array", format.name())))
    }
  }

  // The viewed range as raw bytes (`len` from as_raw is a byte count for
  // every element type). Zero-copy; valid while self is held.
  fn bytes(&self, ctx: &Ctx<'_>, api: &str) -> rquickjs::Result<&[u8]> {
    let raw = match self {
      PixelData::Bytes(a) => a.as_raw(),
      PixelData::Floats(a) => a.as_raw(),
    };
    let raw = raw.ok_or_else(|| throw_str(ctx, &format!("{api}: detached buffer")))?;
    Ok(unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) })
  }
}

// Decode a target create's positional params argument (the live-data half;
// null and undefined both mean none) and its per-target options -
// { textures, buffer, vertexCount, clearColor, render, loadOp, samples,
// filter, wrap, label }, everything optional - into the alloy target spec. `render`
// ("auto" | "manual") and `loadOp` ("clear" | "load") are vocabulary,
// validated here at the boundary like the pipeline words; the
// load-requires-manual invariant is alloy's (Context rejects it).
// The target half of a create's options: size, clear, render mode, loadOp,
// sampling, label. Shared by every mesh-target create.
fn collect_target_half(
  ctx: &Ctx<'_>,
  opts: &Option<Object<'_>>,
  width: u32,
  height: u32,
  api: &str,
) -> rquickjs::Result<alloy::TargetSpec> {
  let mut clear_color = [0f32; 4];
  if let Some(opts) = opts {
    if let Some(arr) = opts.get::<_, Option<Vec<f64>>>("clearColor")? {
      for (slot, v) in clear_color.iter_mut().zip(arr) {
        *slot = v as f32;
      }
    }
  }
  let manual = match opts {
    Some(o) => match o.get::<_, Option<String>>("render")?.as_deref() {
      None | Some("auto") => false,
      Some("manual") => true,
      Some(other) => {
        return Err(throw_str(ctx, &format!("{api}: render must be \"auto\" or \"manual\", got \"{other}\"")))
      }
    },
    None => false,
  };
  let load = match opts {
    Some(o) => match o.get::<_, Option<String>>("loadOp")?.as_deref() {
      None | Some("clear") => false,
      Some("load") => true,
      Some(other) => {
        return Err(throw_str(ctx, &format!("{api}: loadOp must be \"clear\" or \"load\", got \"{other}\"")))
      }
    },
    None => false,
  };
  let samples = match opts {
    Some(o) => match o.get::<_, Option<f64>>("samples")? {
      None => 1,
      Some(n) if n == 1.0 || n == 2.0 || n == 4.0 || n == 8.0 => n as u32,
      Some(n) => return Err(throw_str(ctx, &format!("{api}: samples must be 1, 2, 4 or 8, got {n}"))),
    },
    None => 1,
  };
  let sampler = collect_sampler(ctx, opts, alloy::TextureFormat::Rgba8, api)?;
  let label = collect_label(opts)?;
  Ok(alloy::TargetSpec { width, height, clear_color, sampler, manual, load, samples, label })
}

// The draw-entry half: params (its own argument), textures, buffer, and the
// draw range. Shared by the single-draw creates and addDraw; `pipeline` is 0
// on the fused path (anonymous) and the registry id everywhere else.
fn collect_entry_half(
  ctx: &Ctx<'_>,
  pipeline: u64,
  params: &Option<Object<'_>>,
  opts: &Option<Object<'_>>,
  api: &str,
) -> rquickjs::Result<alloy::DrawSpec> {
  // Params is its own argument (before opts): a params key left in the bag
  // would be silently ignored - the shader renders with defaults - so its
  // presence throws.
  if let Some(o) = opts {
    if o.get::<_, rquickjs::Value>("params").map(|v| !v.is_undefined()).unwrap_or(false) {
      return Err(throw_str(
        ctx,
        &format!("{api}: 'params' is not an option; pass it as its own argument before opts"),
      ));
    }
  }
  let get_obj = |name: &str| -> rquickjs::Result<Option<Object<'_>>> {
    match opts {
      Some(o) => o.get::<_, Option<Object>>(name),
      None => Ok(None),
    }
  };
  let params = match params {
    Some(o) => collect_params(ctx, o, api)?,
    None => Vec::new(),
  };
  let textures = match get_obj("textures")? {
    Some(o) => collect_textures(ctx, &o, api)?,
    None => Vec::new(),
  };
  let buffer = match opts {
    Some(o) => o.get::<_, Option<u64>>("buffer")?.unwrap_or(0),
    None => 0,
  };
  // The index binding: indexBuffer + indexFormat arrive together (the
  // buffer is typeless - any createBuffer result - so the format must be
  // declared, as WebGPU does at setIndexBuffer). Presence switches the
  // range vocabulary below.
  let index_buffer = match opts {
    Some(o) => o.get::<_, Option<u64>>("indexBuffer")?,
    None => None,
  };
  let index_format = match opts {
    Some(o) => o.get::<_, Option<String>>("indexFormat")?,
    None => None,
  };
  let index = match (index_buffer, index_format) {
    (Some(ib), Some(f)) => {
      let format = alloy::IndexFormat::parse(&f).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?;
      Some((ib, format))
    }
    (None, None) => None,
    (Some(_), None) => {
      return Err(throw_str(ctx, &format!("{api}: indexBuffer requires indexFormat (\"uint16\" | \"uint32\")")))
    }
    (None, Some(_)) => return Err(throw_str(ctx, &format!("{api}: indexFormat requires indexBuffer"))),
  };
  // The per-instance buffers, fetched through the pipeline's
  // instanceAttributes; that pairing is validated in alloy against the
  // pipeline mirror (the desc is not in this bag on the split paths).
  // `instanceBuffer` is the single-slot spelling, `instanceBuffers` the
  // per-slot list (index = the attributes' `slot`).
  let instance_buffers = collect_instance_buffers(ctx, opts, api)?;
  // The draw range, in the entry's vocabulary: firstVertex + vertexCount
  // pick the vertices on a plain entry, firstIndex + indexCount pick the
  // indices on an indexed one (WebGPU's spellings for both draw calls);
  // instanceCount repeats the range (gl_InstanceID; 1 = the plain draw, 0
  // draws nothing). The wrong pair throws here, where the fix is visible.
  // A count omitted means "the rest of the buffer" - alloy derives it - and
  // an omitted instanceCount means "one instance per instance-buffer
  // record" (1 without one), so explicit negatives are rejected here, where
  // "omit it" is still meaningful advice; the other sign checks are alloy's.
  let has = |name: &str| -> bool {
    match opts {
      Some(o) => o.get::<_, rquickjs::Value>(name).map(|v| !v.is_undefined()).unwrap_or(false),
      None => false,
    }
  };
  let (first_key, count_key) = if index.is_some() {
    for key in ["firstVertex", "vertexCount"] {
      if has(key) {
        return Err(throw_str(
          ctx,
          &format!("{api}: '{key}' does not apply - the entry is indexed; use firstIndex/indexCount (the range counts indices)"),
        ));
      }
    }
    ("firstIndex", "indexCount")
  } else {
    for key in ["firstIndex", "indexCount"] {
      if has(key) {
        return Err(throw_str(ctx, &format!("{api}: '{key}' needs indexBuffer; use firstVertex/vertexCount")));
      }
    }
    ("firstVertex", "vertexCount")
  };
  let count = match opts {
    Some(o) => o.get::<_, Option<i32>>(count_key)?,
    None => None,
  };
  if let Some(v) = count {
    if v < 0 {
      return Err(throw_str(
        ctx,
        &format!("{api}: {count_key} must be >= 0, got {v} (omit it to draw the whole buffer)"),
      ));
    }
  }
  let instances = match opts {
    Some(o) => o.get::<_, Option<i32>>("instanceCount")?,
    None => None,
  };
  if let Some(v) = instances {
    if v < 0 {
      return Err(throw_str(
        ctx,
        &format!(
          "{api}: instanceCount must be >= 0, got {v} (omit it to draw one instance per instance-buffer record)"
        ),
      ));
    }
  }
  let draw = alloy::DrawRange {
    first_vertex: match opts {
      Some(o) => o.get::<_, Option<i32>>(first_key)?.unwrap_or(0),
      None => 0,
    },
    vertex_count: count.unwrap_or(-1),
    instance_count: instances.unwrap_or(-1),
  };
  let order = collect_instance_order(ctx, opts, api)?;
  Ok(alloy::DrawSpec { pipeline, buffer, index, instance_buffers, draw, params, textures, order })
}

// The instanceOrder option: a field key ({ field }) or a projected key
// ({ position, direction }), float offsets into one instance record, plus
// descending, the key slot ({ slot }, default 0) and the retained-copy
// opt-in ({ retain }). Only the array shape is checked here; the key rules
// (exactly one of the two, offset values, direction values, slot bounds,
// retain with position only, stride fit) are alloy's.
fn collect_instance_order(
  ctx: &Ctx<'_>,
  opts: &Option<Object<'_>>,
  api: &str,
) -> rquickjs::Result<Option<alloy::InstanceOrder>> {
  let obj = match opts {
    Some(o) => o.get::<_, Option<Object>>("instanceOrder")?,
    None => None,
  };
  let Some(o) = obj else {
    return Ok(None);
  };
  let field = o.get::<_, Option<f64>>("field")?;
  let position = o.get::<_, Option<f64>>("position")?;
  let direction = collect_direction(ctx, &o, "direction", api)?;
  let descending = o.get::<_, Option<bool>>("descending")?.unwrap_or(false);
  let slot = o.get::<_, Option<f64>>("slot")?;
  let retain = o.get::<_, Option<bool>>("retain")?.unwrap_or(false);
  alloy::InstanceOrder::parse(field, position, direction, descending, slot, retain)
    .map(Some)
    .map_err(|e| throw_str(ctx, &format!("{api}: {e}")))
}

// An [x, y, z] direction: exactly 3 numbers when the key is present. The
// value semantics (finite, nonzero) are alloy's.
fn collect_direction(ctx: &Ctx<'_>, obj: &Object<'_>, key: &str, api: &str) -> rquickjs::Result<Option<[f32; 3]>> {
  let Some(list) = obj.get::<_, Option<Vec<f64>>>(key)? else {
    return Ok(None);
  };
  if list.len() != 3 {
    return Err(throw_str(ctx, &format!("{api}: {key} must be [x, y, z] (3 numbers), got {}", list.len())));
  }
  Ok(Some([list[0] as f32, list[1] as f32, list[2] as f32]))
}

// The instanceBuffer / instanceBuffers pair at create: one buffer id for
// slot 0, or the per-slot list (index = the attributes' `slot`). Both at
// once contradict each other and throw; neither = no instance buffers.
fn collect_instance_buffers(
  ctx: &Ctx<'_>,
  opts: &Option<Object<'_>>,
  api: &str,
) -> rquickjs::Result<[u64; alloy::MAX_INSTANCE_SLOTS]> {
  let (single, list) = match opts {
    Some(o) => (o.get::<_, Option<u64>>("instanceBuffer")?, o.get::<_, Option<Vec<u64>>>("instanceBuffers")?),
    None => (None, None),
  };
  match (single, list) {
    (Some(_), Some(_)) => Err(throw_str(ctx, &format!("{api}: pass instanceBuffer or instanceBuffers, not both"))),
    (Some(id), None) => Ok([id, 0, 0, 0]),
    (None, Some(list)) => {
      if list.len() > alloy::MAX_INSTANCE_SLOTS {
        return Err(throw_str(
          ctx,
          &format!("{api}: instanceBuffers holds {} buffers; slots are 0..{}", list.len(), alloy::MAX_INSTANCE_SLOTS),
        ));
      }
      let mut ids = [0u64; alloy::MAX_INSTANCE_SLOTS];
      ids[..list.len()].copy_from_slice(&list);
      Ok(ids)
    }
    (None, None) => Ok([0; alloy::MAX_INSTANCE_SLOTS]),
  }
}

fn collect_target_spec(
  ctx: &Ctx<'_>,
  params: &Option<Object<'_>>,
  opts: &Option<Object<'_>>,
  width: u32,
  height: u32,
  api: &str,
) -> rquickjs::Result<(alloy::TargetSpec, alloy::DrawSpec)> {
  Ok((collect_target_half(ctx, opts, width, height, api)?, collect_entry_half(ctx, 0, params, opts, api)?))
}

// Decode a partial draw-range update (setDraw / setDrawRange). Both range
// spellings marshal through untouched; alloy owns the mode rule (the entry
// state lives in its mirror) and rejects the pair that does not match.
// A draw-entry update, both halves: the range keys and the buffer keys
// (see collect_buffer_update), each absent = keep. Alloy applies them as one
// validated transaction.
fn collect_draw_update(ctx: &Ctx<'_>, update: &Object<'_>, api: &str) -> rquickjs::Result<alloy::DrawUpdate> {
  Ok(alloy::DrawUpdate {
    first_vertex: update.get::<_, Option<i32>>("firstVertex")?,
    vertex_count: update.get::<_, Option<i32>>("vertexCount")?,
    first_index: update.get::<_, Option<i32>>("firstIndex")?,
    index_count: update.get::<_, Option<i32>>("indexCount")?,
    instance_count: update.get::<_, Option<i32>>("instanceCount")?,
    buffers: collect_buffer_update(ctx, update, api)?,
    order_direction: collect_direction(ctx, update, "orderDirection", api)?,
  })
}

// The buffer half of a draw-entry update: buffer / indexBuffer + indexFormat
// / instanceBuffer, each absent = keep. The index pair travels together as
// at create; alloy applies the replace-only rule (a present key must name a
// role the entry already fills).
fn collect_buffer_update(ctx: &Ctx<'_>, update: &Object<'_>, api: &str) -> rquickjs::Result<alloy::BufferUpdate> {
  let index = match (update.get::<_, Option<u64>>("indexBuffer")?, update.get::<_, Option<String>>("indexFormat")?) {
    (Some(ib), Some(f)) => {
      let format = alloy::IndexFormat::parse(&f).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?;
      Some((ib, format))
    }
    (None, None) => None,
    (Some(_), None) => {
      return Err(throw_str(ctx, &format!("{api}: indexBuffer requires indexFormat (\"uint16\" | \"uint32\")")))
    }
    (None, Some(_)) => return Err(throw_str(ctx, &format!("{api}: indexFormat requires indexBuffer"))),
  };
  let instance_buffer = update.get::<_, Option<u64>>("instanceBuffer")?;
  let instance_buffers = match update.get::<_, Option<Vec<u64>>>("instanceBuffers")? {
    Some(list) => {
      if instance_buffer.is_some() {
        return Err(throw_str(ctx, &format!("{api}: pass instanceBuffer or instanceBuffers, not both")));
      }
      if list.len() > alloy::MAX_INSTANCE_SLOTS {
        return Err(throw_str(
          ctx,
          &format!("{api}: instanceBuffers holds {} buffers; slots are 0..{}", list.len(), alloy::MAX_INSTANCE_SLOTS),
        ));
      }
      let mut ids = [0u64; alloy::MAX_INSTANCE_SLOTS];
      ids[..list.len()].copy_from_slice(&list);
      Some(ids)
    }
    None => None,
  };
  Ok(alloy::BufferUpdate { buffer: update.get::<_, Option<u64>>("buffer")?, index, instance_buffer, instance_buffers })
}

// Decode createDrawTarget's options: the target half, `depth` (the
// target-OWNED depth storage every entry shares - distinct from the depth
// STATE a pipeline declares on createRenderPipeline), and `textures` (the
// SHARED target-level sampler bindings setTargetTextures drives later - in
// opts like every create's textures). Entry keys throw with a pointer to
// addDraw; draw-state keys with a pointer to createRenderPipeline; a
// `params` key with a pointer to the positional argument (shared
// target-level params, like every create's params).
fn collect_draw_target_spec(
  ctx: &Ctx<'_>,
  opts: &Option<Object<'_>>,
  width: u32,
  height: u32,
  api: &str,
) -> rquickjs::Result<(alloy::TargetSpec, alloy::DepthStorage, Vec<alloy::TextureBinding>, Option<(u64, i32, i32)>)> {
  // `into` makes a sub-target: the parent's id plus the tile's top-left
  // origin `x`/`y` (default 0). Depth is the parent's, so the key is
  // rejected here; render/loadOp/samples are alloy's to reject.
  let into = match opts {
    Some(o) => match o.get::<_, Option<f64>>("into")? {
      Some(parent) => {
        if o.get::<_, rquickjs::Value>("depth").map(|v| !v.is_undefined()).unwrap_or(false) {
          return Err(throw_str(
            ctx,
            &format!("{api}: 'depth' is the parent's on a sub-target; create the parent with it"),
          ));
        }
        let x = o.get::<_, Option<f64>>("x")?.unwrap_or(0.0);
        let y = o.get::<_, Option<f64>>("y")?.unwrap_or(0.0);
        Some((parent as u64, x as i32, y as i32))
      }
      None => None,
    },
    None => None,
  };
  if let Some(o) = opts {
    if o.get::<_, rquickjs::Value>("params").map(|v| !v.is_undefined()).unwrap_or(false) {
      return Err(throw_str(
        ctx,
        &format!("{api}: 'params' is not an option; pass it as its own argument before opts"),
      ));
    }
    for key in [
      "buffer",
      "indexBuffer",
      "indexFormat",
      "instanceBuffer",
      "instanceBuffers",
      "firstVertex",
      "vertexCount",
      "firstIndex",
      "indexCount",
      "instanceCount",
      "instanceOrder",
    ] {
      if o.get::<_, rquickjs::Value>(key).map(|v| !v.is_undefined()).unwrap_or(false) {
        return Err(throw_str(ctx, &format!("{api}: '{key}' is draw-entry state; pass it to addDraw")));
      }
    }
    for key in ["attributes", "instanceAttributes", "topology", "blend", "cull", "depthWrite"] {
      if o.get::<_, rquickjs::Value>(key).map(|v| !v.is_undefined()).unwrap_or(false) {
        return Err(throw_str(ctx, &format!("{api}: '{key}' is pipeline state; pass it to createRenderPipeline")));
      }
    }
  }
  // depth: false (default) | true (a private depth buffer) | "texture" (a
  // sampleable depth texture, reachable through depthTexture(target)).
  let depth = match opts {
    Some(o) => {
      let v: rquickjs::Value<'_> = o.get("depth")?;
      if v.is_undefined() || v.is_null() {
        alloy::DepthStorage::None
      } else if let Some(b) = v.as_bool() {
        if b {
          alloy::DepthStorage::Buffer
        } else {
          alloy::DepthStorage::None
        }
      } else if let Some(s) = v.as_string() {
        match s.to_string()?.as_str() {
          "texture" => alloy::DepthStorage::Texture,
          other => {
            return Err(throw_str(ctx, &format!("{api}: depth must be a boolean or \"texture\", got \"{other}\"")));
          }
        }
      } else {
        return Err(throw_str(ctx, &format!("{api}: depth must be a boolean or \"texture\"")));
      }
    }
    None => alloy::DepthStorage::None,
  };
  let textures = match opts {
    Some(o) => match o.get::<_, Option<Object<'_>>>("textures")? {
      Some(t) => collect_textures(ctx, &t, api)?,
      None => Vec::new(),
    },
    None => Vec::new(),
  };
  Ok((collect_target_half(ctx, opts, width, height, api)?, depth, textures, into))
}

// Decode the draw-state options of createRenderPipeline and
// createPipelineTexture -
// { attributes: [{name, format}], instanceAttributes: [{name, format}],
// topology, blend, depth, depthWrite }, everything optional - into the typed
// alloy desc. The vocabulary parses here, at the boundary, so `blend:
// "addd"` (or an invalid depth/depthWrite combination) throws at the call
// site instead of failing on the raster thread.
fn collect_pipeline_desc(ctx: &Ctx<'_>, opts: &Option<Object<'_>>, api: &str) -> rquickjs::Result<alloy::PipelineDesc> {
  let collect_layout = |key: &str| -> rquickjs::Result<Vec<(String, alloy::AttrFormat)>> {
    let mut attributes: Vec<(String, alloy::AttrFormat)> = Vec::new();
    if let Some(opts) = opts {
      if let Some(arr) = opts.get::<_, Option<Array>>(key)? {
        for item in arr.iter::<Object>() {
          let entry = item?;
          let name: String = entry.get("name")?;
          let format: String = entry.get("format")?;
          let format = alloy::AttrFormat::parse(&format).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?;
          attributes.push((name, format));
        }
      }
    }
    Ok(attributes)
  };
  let attributes = collect_layout("attributes")?;
  // Instance attributes additionally take `slot` (default 0): which entry
  // of the entry's instanceBuffers list the attribute fetches from.
  // Attributes sharing a slot interleave into one record; slot density and
  // the cap are validated in alloy.
  let mut instance_attributes: Vec<(String, alloy::AttrFormat, u32)> = Vec::new();
  if let Some(o) = opts {
    if let Some(arr) = o.get::<_, Option<Array>>("instanceAttributes")? {
      for item in arr.iter::<Object>() {
        let entry = item?;
        let name: String = entry.get("name")?;
        let format: String = entry.get("format")?;
        let format = alloy::AttrFormat::parse(&format).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?;
        let slot = entry.get::<_, Option<i32>>("slot")?.unwrap_or(0);
        if slot < 0 {
          return Err(throw_str(ctx, &format!("{api}: instance attribute '{name}' slot must be >= 0, got {slot}")));
        }
        instance_attributes.push((name, format, slot as u32));
      }
    }
  }
  let topology = match opts {
    Some(o) => match o.get::<_, Option<String>>("topology")? {
      Some(s) => alloy::Topology::parse(&s).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?,
      None => alloy::Topology::Triangles,
    },
    None => alloy::Topology::Triangles,
  };
  let blend = match opts {
    Some(o) => match o.get::<_, Option<String>>("blend")? {
      Some(s) => alloy::parse_blend(&s).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?,
      None => None,
    },
    None => None,
  };
  let cull = match opts {
    Some(o) => match o.get::<_, Option<String>>("cull")? {
      Some(s) => alloy::parse_cull(&s).map_err(|e| throw_str(ctx, &format!("{api}: {e}")))?,
      None => None,
    },
    None => None,
  };
  let (depth, depth_write) = match opts {
    Some(o) => (o.get::<_, Option<bool>>("depth")?.unwrap_or(false), o.get::<_, Option<bool>>("depthWrite")?),
    None => (false, None),
  };
  let depth = match (depth, depth_write) {
    (true, write) => Some(alloy::DepthState { write: write.unwrap_or(true) }),
    (false, Some(false)) => {
      return Err(throw_str(
        ctx,
        &format!("{api}: depthWrite: false requires depth: true (there is no depth buffer to leave unwritten)"),
      ))
    }
    (false, _) => None,
  };
  Ok(alloy::PipelineDesc { attributes, instance_attributes, topology, blend, depth, cull })
}

// The migration guard for the split object model: draw state belongs to
// createRenderPipeline, and a target create silently ignoring these keys is
// exactly the bug class the split removes - so their presence throws.
fn reject_pipeline_keys(ctx: &Ctx<'_>, opts: &Option<Object<'_>>, api: &str) -> rquickjs::Result<()> {
  if let Some(o) = opts {
    for key in ["attributes", "instanceAttributes", "topology", "blend", "cull", "depth", "depthWrite"] {
      if o.get::<_, rquickjs::Value>(key).map(|v| !v.is_undefined()).unwrap_or(false) {
        return Err(throw_str(ctx, &format!("{api}: '{key}' is pipeline state; pass it to createRenderPipeline")));
      }
    }
  }
  Ok(())
}

/// Store the texture plugin state (alloy context, platform, and the created-id
/// set for reload cleanup) in userdata, before any module import. The
/// `flux:gpu` surface is registered separately via `module_override`.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx
    .store_userdata(TextureState(Rc::new(TextureInner {
      gui: super::gui(ctx),
      created: RefCell::new(HashSet::new()),
      created_buffers: RefCell::new(HashSet::new()),
      open_buffer_writes: RefCell::new(HashMap::new()),
      created_programs: RefCell::new(HashSet::new()),
      created_pipelines: RefCell::new(HashSet::new()),
      created_stages: RefCell::new(HashSet::new()),
      capture_settle: RefCell::new(Vec::new()),
    })))
    .expect("store texture state");
}

/// The `flux:gpu` module: texture creation/upload/destruction and fragment
/// shaders. Texture ids are the public token (used as `<texture src>` and shader
/// sampler inputs), so there is no handle to hide here. Every export is a free
/// function reading the plugin state from userdata (`state`); the ones that
/// return a `'js`-bound handle name the lifetime explicitly so it unifies with
/// the `Ctx` argument (see flux/CLAUDE.md, "Ctx and the 'js lifetime").
pub struct GpuModule;

impl ModuleDef for GpuModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createTexture")?;
    decl.declare("createCubeTexture")?;
    decl.declare("createMutableTexture")?;
    decl.declare("uploadTexture")?;
    decl.declare("resizeTexture")?;
    decl.declare("destroyTexture")?;
    decl.declare("createShaderTexture")?;
    decl.declare("compileShader")?;
    decl.declare("linkProgram")?;
    decl.declare("destroyShader")?;
    decl.declare("createShaderTarget")?;
    decl.declare("createRenderPipeline")?;
    decl.declare("destroyRenderPipeline")?;
    decl.declare("destroyProgram")?;
    decl.declare("programAttributes")?;
    decl.declare("createPipelineTexture")?;
    decl.declare("createBuffer")?;
    decl.declare("beginBufferWrite")?;
    decl.declare("endBufferWrite")?;
    decl.declare("writeBuffer")?;
    decl.declare("destroyBuffer")?;
    decl.declare("setDraw")?;
    decl.declare("createDrawTarget")?;
    decl.declare("depthTexture")?;
    decl.declare("addDraw")?;
    decl.declare("removeDraw")?;
    decl.declare("setDrawParams")?;
    decl.declare("setTargetParams")?;
    decl.declare("setTargetTextures")?;
    decl.declare("setTargetSize")?;
    decl.declare("setTargetRect")?;
    decl.declare("setDrawTextures")?;
    decl.declare("setDrawRange")?;
    decl.declare("setDrawBuffers")?;
    decl.declare("setDrawOrder")?;
    decl.declare("renderTarget")?;
    decl.declare("copyTexture")?;
    decl.declare("captureSnapshot")?;
    decl.declare("readTexture")?;
    decl.declare("limits")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("createTexture", Function::new(ctx.clone(), create_texture)?)?;
    exports.export("createCubeTexture", Function::new(ctx.clone(), create_cube_texture)?)?;
    exports.export("createMutableTexture", Function::new(ctx.clone(), create_mutable_texture)?)?;
    exports.export("uploadTexture", Function::new(ctx.clone(), upload_texture)?)?;
    exports.export("resizeTexture", Function::new(ctx.clone(), resize_texture)?)?;
    exports.export("destroyTexture", Function::new(ctx.clone(), destroy_texture)?)?;
    exports.export("createShaderTexture", Function::new(ctx.clone(), create_shader_texture)?)?;
    exports.export("compileShader", Function::new(ctx.clone(), compile_shader)?)?;
    exports.export("linkProgram", Function::new(ctx.clone(), link_program)?)?;
    exports.export("destroyShader", Function::new(ctx.clone(), destroy_shader)?)?;
    exports.export("createShaderTarget", Function::new(ctx.clone(), create_shader_target)?)?;
    exports.export("createRenderPipeline", Function::new(ctx.clone(), create_render_pipeline)?)?;
    exports.export("destroyRenderPipeline", Function::new(ctx.clone(), destroy_render_pipeline)?)?;
    exports.export("destroyProgram", Function::new(ctx.clone(), destroy_program)?)?;
    exports.export("programAttributes", Function::new(ctx.clone(), program_attributes)?)?;
    exports.export("createPipelineTexture", Function::new(ctx.clone(), create_pipeline_texture)?)?;
    exports.export("createBuffer", Function::new(ctx.clone(), create_buffer)?)?;
    exports.export("beginBufferWrite", Function::new(ctx.clone(), begin_buffer_write)?)?;
    exports.export("endBufferWrite", Function::new(ctx.clone(), end_buffer_write)?)?;
    exports.export("writeBuffer", Function::new(ctx.clone(), write_buffer)?)?;
    exports.export("destroyBuffer", Function::new(ctx.clone(), destroy_buffer)?)?;
    exports.export("setDraw", Function::new(ctx.clone(), set_draw)?)?;
    exports.export("createDrawTarget", Function::new(ctx.clone(), create_draw_target)?)?;
    exports.export("depthTexture", Function::new(ctx.clone(), depth_texture)?)?;
    exports.export("addDraw", Function::new(ctx.clone(), add_draw)?)?;
    exports.export("removeDraw", Function::new(ctx.clone(), remove_draw)?)?;
    exports.export("setDrawParams", Function::new(ctx.clone(), set_draw_params)?)?;
    exports.export("setTargetParams", Function::new(ctx.clone(), set_target_params)?)?;
    exports.export("setTargetTextures", Function::new(ctx.clone(), set_target_textures)?)?;
    exports.export("setTargetSize", Function::new(ctx.clone(), set_target_size)?)?;
    exports.export("setTargetRect", Function::new(ctx.clone(), set_target_rect)?)?;
    exports.export("setDrawTextures", Function::new(ctx.clone(), set_draw_textures)?)?;
    exports.export("setDrawRange", Function::new(ctx.clone(), set_draw_range)?)?;
    exports.export("setDrawBuffers", Function::new(ctx.clone(), set_draw_buffers)?)?;
    exports.export("setDrawOrder", Function::new(ctx.clone(), set_draw_order)?)?;
    exports.export("renderTarget", Function::new(ctx.clone(), render_target)?)?;
    exports.export("copyTexture", Function::new(ctx.clone(), copy_texture)?)?;
    exports.export("captureSnapshot", Function::new(ctx.clone(), capture_snapshot)?)?;
    exports.export("readTexture", Function::new(ctx.clone(), read_texture)?)?;

    // The device ceilings, process constants queried once at raster startup
    // (module evaluate runs at import time, warming alloy's UI-side cache
    // before any app code validates against it). Exported as a plain object:
    // there is nothing to call, the values never change.
    let limits = state(ctx).gui.alloy.gpu_limits();
    let limits_obj = Object::new(ctx.clone())?;
    limits_obj.set("maxTextureSize", limits.max_texture_size)?;
    limits_obj.set("maxCubeMapSize", limits.max_cube_map_size)?;
    limits_obj.set("maxTextureUnits", limits.max_texture_units)?;
    limits_obj.set("maxVertexAttribs", limits.max_vertex_attribs)?;
    limits_obj.set("maxAnisotropy", limits.max_anisotropy)?;
    limits_obj.set("maxVertexUniformVectors", limits.max_vertex_uniform_vectors)?;
    exports.export("limits", limits_obj)?;
    Ok(())
  }
}

fn create_texture(
  ctx: Ctx<'_>,
  data: Value<'_>,
  width: u32,
  height: u32,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  let format = collect_format(&ctx, &opts.0, "createTexture")?;
  let data = PixelData::collect(&ctx, data, format, "createTexture")?;
  let pixels = data.bytes(&ctx, "createTexture")?;
  let expected = format.byte_len(width, height);
  if pixels.len() != expected {
    return Err(throw_str(
      &ctx,
      &format!("createTexture: expected {expected} bytes ({}), got {}", format.name(), pixels.len()),
    ));
  }
  let sampler = collect_sampler(&ctx, &opts.0, format, "createTexture")?;
  let label = collect_label(&opts.0)?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_texture_from_pixels(width, height, pixels, sampler, format, label)
    .map_err(|e| throw_str(&ctx, &format!("createTexture: {e}")))?;
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// A cube map: six faces in GL order (+X, -X, +Y, -Y, +Z, -Z), each one
// frame of `size` x `size` at the declared format (same view-type rule as
// createTexture), sampled by direction through a `samplerCube`. Sampler
// options are the createTexture set; `wrap` parses but has no effect
// (GLES 3.0 cube filtering is seamless).
fn create_cube_texture(ctx: Ctx<'_>, faces: Array<'_>, size: u32, opts: OptArg<Object<'_>>) -> rquickjs::Result<u64> {
  let format = collect_format(&ctx, &opts.0, "createCubeTexture")?;
  if faces.len() != alloy::CUBE_FACES {
    return Err(throw_str(
      &ctx,
      &format!("createCubeTexture: faces must be 6 buffers (+X, -X, +Y, -Y, +Z, -Z), got {}", faces.len()),
    ));
  }
  let expected = format.byte_len(size, size);
  let mut pixels = Vec::with_capacity(alloy::CUBE_FACES);
  for (i, face) in faces.iter::<Value>().enumerate() {
    let data = PixelData::collect(&ctx, face?, format, "createCubeTexture")?;
    let bytes = data.bytes(&ctx, "createCubeTexture")?;
    if bytes.len() != expected {
      return Err(throw_str(
        &ctx,
        &format!(
          "createCubeTexture: face {i} is {} bytes, expected {expected} ({size}x{size} {})",
          bytes.len(),
          format.name()
        ),
      ));
    }
    pixels.push(bytes.to_vec());
  }
  let sampler = collect_sampler(&ctx, &opts.0, format, "createCubeTexture")?;
  let label = collect_label(&opts.0)?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_cube_texture(size, pixels, sampler, format, label)
    .map_err(|e| throw_str(&ctx, &format!("createCubeTexture: {e}")))?;
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// A mutable texture is created exactly like an immutable one; "mutable" only
// signals intent to update it later via uploadTexture. The seed buffer may be
// larger than one frame (uploadTexture takes an offset), so only require that
// the first frame fits.
fn create_mutable_texture(
  ctx: Ctx<'_>,
  data: Value<'_>,
  width: u32,
  height: u32,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  let format = collect_format(&ctx, &opts.0, "createMutableTexture")?;
  let data = PixelData::collect(&ctx, data, format, "createMutableTexture")?;
  let all = data.bytes(&ctx, "createMutableTexture")?;
  let frame_size = format.byte_len(width, height);
  if all.len() < frame_size {
    return Err(throw_str(
      &ctx,
      &format!("createMutableTexture: need at least {frame_size} bytes ({}), got {}", format.name(), all.len()),
    ));
  }
  let sampler = collect_sampler(&ctx, &opts.0, format, "createMutableTexture")?;
  let label = collect_label(&opts.0)?;
  let pixels = &all[..frame_size];
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_texture_from_pixels(width, height, pixels, sampler, format, label)
    .map_err(|e| throw_str(&ctx, &format!("createMutableTexture: {e}")))?;
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// The caller passes the pixel buffer on every upload (it already holds it),
// so nothing is pinned on the Rust side. `data` may hold multiple frames;
// `offset` selects the one to upload. Reading the buffer is zero-copy.
fn upload_texture(ctx: Ctx<'_>, id: u64, data: Value<'_>, offset: OptArg<usize>) -> rquickjs::Result<()> {
  let st = state(&ctx);
  let format = st.gui.alloy.texture_format(id).map_err(|e| throw_str(&ctx, &format!("uploadTexture: {e}")))?;
  let data = PixelData::collect(&ctx, data, format, "uploadTexture")?;
  let pixels = data.bytes(&ctx, "uploadTexture")?;
  st.gui
    .alloy
    .update_texture(id, pixels, offset.0.unwrap_or(0))
    .map_err(|e| throw_str(&ctx, &format!("uploadTexture: {e}")))?;
  // New texture content changes the screen without any tree mutation.
  st.gui.platform.request_frame();
  Ok(())
}

// An id-stable resize: replaces the texture's storage at the same id, so
// `<texture src>` references and shader sampler bindings stay valid (the
// sampling shaders re-render). `data` seeds the new size and, like
// createMutableTexture, must hold at least one frame. Alloy validates the
// id (render targets are rejected there; those resize via setTargetSize).
fn resize_texture(ctx: Ctx<'_>, id: u64, data: Value<'_>, width: u32, height: u32) -> rquickjs::Result<()> {
  let st = state(&ctx);
  let format = st.gui.alloy.texture_format(id).map_err(|e| throw_str(&ctx, &format!("resizeTexture: {e}")))?;
  let data = PixelData::collect(&ctx, data, format, "resizeTexture")?;
  let pixels = data.bytes(&ctx, "resizeTexture")?;
  st.gui
    .alloy
    .resize_texture(id, width, height, pixels)
    .map_err(|e| throw_str(&ctx, &format!("resizeTexture: {e}")))?;
  // The replacement changes the screen without any tree mutation.
  st.gui.platform.request_frame();
  Ok(())
}

fn create_shader_texture(
  ctx: Ctx<'_>,
  fragment_src: String,
  width: u32,
  height: u32,
  params: Option<Object<'_>>,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  let params = match &params {
    Some(o) => collect_params(&ctx, o, "createShaderTexture")?,
    None => Vec::new(),
  };
  let textures = match &opts.0 {
    Some(o) => match o.get::<_, Option<Object>>("textures")? {
      Some(t) => collect_textures(&ctx, &t, "createShaderTexture")?,
      None => Vec::new(),
    },
    None => Vec::new(),
  };
  let sampler = collect_sampler(&ctx, &opts.0, alloy::TextureFormat::Rgba8, "createShaderTexture")?;
  let label = collect_label(&opts.0)?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_shader_texture(width, height, &fragment_src, &params, &textures, sampler, label)
    .map_err(|e| throw_str(&ctx, &format!("createShaderTexture: {e}")))?;
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// createPipelineTexture(vertexSrc, fragmentSrc, width, height, params?,
// opts?) -> texture id: the fused convenience, taking the draw-state
// options AND the target options in one bag (params is its own argument).
// Vocabulary parses here; alloy validates the rest.
fn create_pipeline_texture(
  ctx: Ctx<'_>,
  vertex_src: String,
  fragment_src: String,
  width: u32,
  height: u32,
  params: Option<Object<'_>>,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  let pipeline = collect_pipeline_desc(&ctx, &opts.0, "createPipelineTexture")?;
  let (target, entry) = collect_target_spec(&ctx, &params, &opts.0, width, height, "createPipelineTexture")?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_pipeline_texture(alloy::PipelineSpec { vertex_src, fragment_src, pipeline, target, entry })
    .map_err(|e| throw_str(&ctx, &format!("createPipelineTexture: {e}")))?;
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// createRenderPipeline(program, opts?) -> pipeline id: pair a linked
// program with draw state. Its own id space (like programs and buffers);
// creating one compiles nothing.
fn create_render_pipeline(ctx: Ctx<'_>, program: u64, opts: OptArg<Object<'_>>) -> rquickjs::Result<u64> {
  let desc = collect_pipeline_desc(&ctx, &opts.0, "createRenderPipeline")?;
  let label = collect_label(&opts.0)?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_render_pipeline(program, desc, label)
    .map_err(|e| throw_str(&ctx, &format!("createRenderPipeline: {e}")))?;
  st.created_pipelines.borrow_mut().insert(id);
  Ok(id)
}

fn destroy_render_pipeline(ctx: Ctx<'_>, id: u64) {
  let st = state(&ctx);
  st.created_pipelines.borrow_mut().remove(&id);
  st.gui.alloy.destroy_render_pipeline(id);
}

// Raw stage compile: complete GLSL ES by default, the standard header on
// explicit request. Compile errors throw here, at a call site the app
// chose.
fn compile_shader(ctx: Ctx<'_>, stage: String, source: String, opts: OptArg<Object<'_>>) -> rquickjs::Result<u64> {
  let stage = alloy::ShaderStage::parse(&stage).map_err(|e| throw_str(&ctx, &format!("compileShader: {e}")))?;
  let header = match &opts.0 {
    Some(o) => o.get::<_, Option<bool>>("header")?.unwrap_or(false),
    None => false,
  };
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .compile_shader_stage(stage, &source, header)
    .map_err(|e| throw_str(&ctx, &format!("compileShader: {e}")))?;
  st.created_stages.borrow_mut().insert(id);
  Ok(id)
}

// Link two compiled stages into a program: the handle targets (and later
// the window effect) are created from. Link errors throw here.
fn link_program(ctx: Ctx<'_>, vertex: u64, fragment: u64, opts: OptArg<Object<'_>>) -> rquickjs::Result<u64> {
  let label = collect_label(&opts.0)?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .link_shader_program(vertex, fragment, label)
    .map_err(|e| throw_str(&ctx, &format!("linkProgram: {e}")))?;
  st.created_programs.borrow_mut().insert(id);
  Ok(id)
}

fn destroy_shader(ctx: Ctx<'_>, id: u64) {
  let st = state(&ctx);
  st.created_stages.borrow_mut().remove(&id);
  st.gui.alloy.destroy_shader_stage(id);
}

// createShaderTarget(pipeline, width, height, params?, opts?) -> texture
// id: the per-target half over a render pipeline. Draw-state keys in opts
// throw (they belong to createRenderPipeline).
fn create_shader_target(
  ctx: Ctx<'_>,
  pipeline: u64,
  width: u32,
  height: u32,
  params: Option<Object<'_>>,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  reject_pipeline_keys(&ctx, &opts.0, "createShaderTarget")?;
  let (spec, entry) = collect_target_spec(&ctx, &params, &opts.0, width, height, "createShaderTarget")?;
  let st = state(&ctx);
  let id = st
    .gui
    .alloy
    .create_shader_target(pipeline, spec, entry)
    .map_err(|e| throw_str(&ctx, &format!("createShaderTarget: {e}")))?;
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// programAttributes(program) -> [{ name, format }]: the vertex attributes
// the linked program actually reads, as the compiler left them - the
// list a pipeline's attributes + instanceAttributes must cover. Answered
// from the UI-side mirror, no raster round trip.
fn program_attributes<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Array<'js>> {
  let table =
    state(&ctx).gui.alloy.program_attributes(id).map_err(|e| throw_str(&ctx, &format!("programAttributes: {e}")))?;
  let arr = Array::new(ctx.clone())?;
  for (i, (name, format)) in table.iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("name", name.as_str())?;
    obj.set("format", format.name())?;
    arr.set(i, obj)?;
  }
  Ok(arr)
}

fn destroy_program(ctx: Ctx<'_>, id: u64) {
  let st = state(&ctx);
  st.created_programs.borrow_mut().remove(&id);
  st.gui.alloy.destroy_shader_program(id);
}

// createBuffer(data | byteLength): bytes seed the buffer; a number makes
// a zeroed one, the natural create for buffers filled through the write
// lease (beginBufferWrite) where initial contents would be dead weight.
fn create_buffer(ctx: Ctx<'_>, data: Value<'_>, opts: OptArg<Object<'_>>) -> rquickjs::Result<u64> {
  let label = collect_label(&opts.0)?;
  let st = state(&ctx);
  let id = if let Some(n) = data.as_number() {
    if !(n.is_finite() && n >= 0.0 && n.fract() == 0.0) {
      return Err(throw_str(&ctx, &format!("createBuffer: byteLength must be a non-negative integer, got {n}")));
    }
    st.gui
      .alloy
      .create_gpu_buffer_zeroed(n as usize, label)
      .map_err(|e| throw_str(&ctx, &format!("createBuffer: {e}")))?
  } else {
    let data = TypedArray::<u8>::from_value(data)
      .map_err(|_| throw_str(&ctx, "createBuffer: expected a Uint8Array or a byteLength number"))?;
    let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "createBuffer: detached buffer"))?;
    let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
    st.gui.alloy.create_gpu_buffer(bytes, label).map_err(|e| throw_str(&ctx, &format!("createBuffer: {e}")))?
  };
  st.created_buffers.borrow_mut().insert(id);
  Ok(id)
}

// The zero-copy write pair. begin hands JS a view over a runtime-owned
// staging block (contents UNSPECIFIED - a recycled block holds what was
// published the time before last, so fill everything you publish); end
// detaches the view FIRST, then publishes the prefix by moving the block
// to the raster thread - no copy anywhere on the CPU path. end always
// closes the lease, error or not; byteLength 0 cancels.
// begin returns an ArrayBuffer whose 'js lifetime must unify with the Ctx
// arg, hence the explicit lifetime (the readTexture/captureSnapshot rule).
fn begin_buffer_write<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<ArrayBuffer<'js>> {
  let st = state(&ctx);
  if st.open_buffer_writes.borrow().contains_key(&id) {
    return Err(throw_str(&ctx, &format!("beginBufferWrite: buffer {id} already has an open write")));
  }
  let (ptr, len) =
    st.gui.alloy.begin_buffer_write(id).map_err(|e| throw_str(&ctx, &format!("beginBufferWrite: {e}")))?;
  let view = match array_buffer_over(&ctx, ptr, len) {
    Ok(view) => view,
    Err(e) => {
      // The lease is open in alloy but no JS view exists: cancel it so the
      // id is not wedged.
      st.gui.alloy.end_buffer_write(id, 0).ok();
      return Err(e);
    }
  };
  let saved = Persistent::save(&ctx, view.clone());
  st.open_buffer_writes.borrow_mut().insert(id, OpenLease { view: saved, size: len });
  Ok(view)
}

fn end_buffer_write(ctx: Ctx<'_>, id: u64, byte_length: OptArg<usize>) -> rquickjs::Result<()> {
  let st = state(&ctx);
  let lease = st
    .open_buffer_writes
    .borrow_mut()
    .remove(&id)
    .ok_or_else(|| throw_str(&ctx, &format!("endBufferWrite: buffer {id} has no open write")))?;
  // Detach before the block moves: a JS write after this point lands in
  // a zero-length view, never in bytes the raster thread is reading.
  if let Ok(mut view) = lease.view.restore(&ctx) {
    view.detach();
  }
  let len = byte_length.0.unwrap_or(lease.size);
  st.gui.alloy.end_buffer_write(id, len).map_err(|e| throw_str(&ctx, &format!("endBufferWrite: {e}")))?;
  if len > 0 {
    // New buffer contents change the screen without any tree mutation.
    st.gui.platform.request_frame();
  }
  Ok(())
}

// A write re-renders the pipelines drawing from the buffer (alloy does
// that), so the screen changes without any tree mutation: request a frame.
fn write_buffer(ctx: Ctx<'_>, id: u64, data: TypedArray<'_, u8>, offset: OptArg<usize>) -> rquickjs::Result<()> {
  let raw = data.as_raw().ok_or_else(|| throw_str(&ctx, "writeBuffer: detached buffer"))?;
  let bytes = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) };
  let st = state(&ctx);
  st.gui
    .alloy
    .write_gpu_buffer(id, bytes, offset.0.unwrap_or(0))
    .map_err(|e| throw_str(&ctx, &format!("writeBuffer: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

fn destroy_buffer(ctx: Ctx<'_>, id: u64) {
  let st = state(&ctx);
  // A destroy mid-lease detaches the JS view before the block dies with
  // the Context-side lease state.
  if let Some(lease) = st.open_buffer_writes.borrow_mut().remove(&id) {
    if let Ok(mut view) = lease.view.restore(&ctx) {
      view.detach();
    }
  }
  st.created_buffers.borrow_mut().remove(&id);
  st.gui.alloy.destroy_gpu_buffer(id);
}

// Partial draw-entry update: keys present overwrite, absent keys keep
// their current value (the params merge rule). Both range spellings
// marshal through; alloy rejects the pair that does not match the
// entry's mode and validates the merged range against the mirrored
// fetch bound, all at this call site. Buffer keys swap the entry's
// buffers (replace-only, see Context::update_draw) in the same
// transaction, so one call can grow a buffer and extend the range into
// it, and a rejected call changes nothing.
fn set_draw(ctx: Ctx<'_>, id: u64, draw: Object<'_>) -> rquickjs::Result<()> {
  let update = collect_draw_update(&ctx, &draw, "setDraw")?;
  let st = state(&ctx);
  st.gui.alloy.set_draw(id, update).map_err(|e| throw_str(&ctx, &format!("setDraw: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// createDrawTarget(width, height, params?, opts?) -> texture id: a mesh
// target whose contents are an ordered, mutable list of draws (addDraw /
// removeDraw), over color plus optional target-owned depth storage.
// `params` seeds the SHARED (target-level) params setTargetParams drives
// later - positional like every create's params - and `opts.textures`
// seeds the shared sampler bindings setTargetTextures drives; the rest
// of the options are the target half only, entries carry everything
// else.
fn create_draw_target(
  ctx: Ctx<'_>,
  width: u32,
  height: u32,
  params: Option<Object<'_>>,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  let (spec, depth, textures, into) = collect_draw_target_spec(&ctx, &opts.0, width, height, "createDrawTarget")?;
  let params = match &params {
    Some(o) => collect_params(&ctx, o, "createDrawTarget")?,
    None => Vec::new(),
  };
  let st = state(&ctx);
  let id = match into {
    Some((parent, x, y)) => st.gui.alloy.create_sub_target(parent, x, y, spec),
    None => st.gui.alloy.create_draw_target(spec, depth),
  }
  .map_err(|e| throw_str(&ctx, &format!("createDrawTarget: {e}")))?;
  if !params.is_empty() {
    st.gui.alloy.set_target_params(id, &params).map_err(|e| throw_str(&ctx, &format!("createDrawTarget: {e}")))?;
  }
  if !textures.is_empty() {
    st.gui.alloy.set_target_textures(id, &textures).map_err(|e| throw_str(&ctx, &format!("createDrawTarget: {e}")))?;
  }
  st.created.borrow_mut().insert(id);
  Ok(id)
}

// depthTexture(target) -> texture id: the depth texture of a draw target
// created with depth: "texture" - a sampler-only id (bind it anywhere a
// texture binds; it dies with its target, destroyTexture on it throws).
fn depth_texture(ctx: Ctx<'_>, target: u64) -> rquickjs::Result<u64> {
  state(&ctx).gui.alloy.depth_texture(target).map_err(|e| throw_str(&ctx, &format!("depthTexture: {e}")))
}

// addDraw(target, pipeline, params?, opts?) -> draw id: add a draw
// entry (same shape as createShaderTarget's per-entry arguments),
// appended, or inserted via opts.before. The returned id is stable
// across add/remove - the handle every per-entry update takes. Alloy
// validates everything at this call site.
fn add_draw(
  ctx: Ctx<'_>,
  target: u64,
  pipeline: u64,
  params: Option<Object<'_>>,
  opts: OptArg<Object<'_>>,
) -> rquickjs::Result<u64> {
  reject_pipeline_keys(&ctx, &opts.0, "addDraw")?;
  let entry = collect_entry_half(&ctx, pipeline, &params, &opts.0, "addDraw")?;
  let before = match &opts.0 {
    Some(o) => o.get::<_, Option<u64>>("before")?,
    None => None,
  };
  let st = state(&ctx);
  let id = st.gui.alloy.add_draw(target, entry, before).map_err(|e| throw_str(&ctx, &format!("addDraw: {e}")))?;
  st.gui.platform.request_frame();
  Ok(id)
}

// setDrawOrder(target, order): reorder the list to a full permutation of
// the current entry ids - the sorting verb.
fn set_draw_order(ctx: Ctx<'_>, target: u64, order: Vec<u64>) -> rquickjs::Result<()> {
  let st = state(&ctx);
  st.gui.alloy.set_draw_order(target, &order).map_err(|e| throw_str(&ctx, &format!("setDrawOrder: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

fn remove_draw(ctx: Ctx<'_>, target: u64, draw: u64) -> rquickjs::Result<()> {
  let st = state(&ctx);
  st.gui.alloy.remove_draw(target, draw).map_err(|e| throw_str(&ctx, &format!("removeDraw: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Per-entry params update: setTargetParams addressed to one draw entry.
fn set_draw_params(ctx: Ctx<'_>, target: u64, draw: u64, params: Object<'_>) -> rquickjs::Result<()> {
  let params = collect_params(&ctx, &params, "setDrawParams")?;
  let st = state(&ctx);
  st.gui.alloy.set_draw_params(target, draw, &params).map_err(|e| throw_str(&ctx, &format!("setDrawParams: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Target-level params update on any target kind; alloy routes. On a
// single-program target (fragment texture, fixed pipeline target) these
// are the one pass's params, validated strictly. On a draw target they
// are the SHARED params: values every entry reads (a camera's
// view-projection), applied before each entry's own params - an entry
// naming the same uniform overrides the shared value - and coverage may
// be partial down to zero (the apply skips undeclared names; a name no
// entry declares yet is stored for entries added later). Arity is
// validated wherever a name IS declared.
fn set_target_params(ctx: Ctx<'_>, target: u64, params: Object<'_>) -> rquickjs::Result<()> {
  let params = collect_params(&ctx, &params, "setTargetParams")?;
  let st = state(&ctx);
  st.gui.alloy.set_target_params(target, &params).map_err(|e| throw_str(&ctx, &format!("setTargetParams: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Target-level sampler rebind on any target kind; alloy routes (the
// sampler analog of setTargetParams). On a draw target these are the
// SHARED bindings: sources every entry reads (an environment map, a
// LUT), applied where an entry's program declares the name and its own
// bindings do not override it. Alloy validates at this call site
// (coverage, sources, unit budget, cycles).
fn set_target_textures(ctx: Ctx<'_>, target: u64, textures: Object<'_>) -> rquickjs::Result<()> {
  let textures = collect_textures(&ctx, &textures, "setTargetTextures")?;
  let st = state(&ctx);
  st.gui
    .alloy
    .set_target_textures(target, &textures)
    .map_err(|e| throw_str(&ctx, &format!("setTargetTextures: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Resize a target of any kind in place: the id, compiled programs,
// last-applied params, sampler bindings, and draw state all carry over,
// and the output re-renders at the new size.
fn set_target_size(ctx: Ctx<'_>, id: u64, width: u32, height: u32) -> rquickjs::Result<()> {
  let st = state(&ctx);
  st.gui.alloy.resize_target(id, width, height).map_err(|e| throw_str(&ctx, &format!("setTargetSize: {e}")))?;
  // New target output changes the screen without any tree mutation.
  st.gui.platform.request_frame();
  Ok(())
}

// setTargetRect(id, { x, y, width, height }): move and resize a
// sub-target's rectangle in its parent (top-left origin). Every key is
// required; the parent re-renders in full.
fn set_target_rect(ctx: Ctx<'_>, id: u64, rect: Object<'_>) -> rquickjs::Result<()> {
  let mut values = [0f64; 4];
  for (slot, key) in values.iter_mut().zip(["x", "y", "width", "height"]) {
    *slot = rect
      .get::<_, Option<f64>>(key)?
      .ok_or_else(|| throw_str(&ctx, &format!("setTargetRect: '{key}' is required")))?;
  }
  let [x, y, width, height] = values;
  if width < 1.0 || height < 1.0 {
    return Err(throw_str(&ctx, "setTargetRect: width and height must be at least 1"));
  }
  let st = state(&ctx);
  st.gui
    .alloy
    .set_target_rect(id, x as i32, y as i32, width as u32, height as u32)
    .map_err(|e| throw_str(&ctx, &format!("setTargetRect: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Per-entry sampler rebind: setTargetTextures addressed to one draw entry.
fn set_draw_textures(ctx: Ctx<'_>, target: u64, draw: u64, textures: Object<'_>) -> rquickjs::Result<()> {
  let textures = collect_textures(&ctx, &textures, "setDrawTextures")?;
  let st = state(&ctx);
  st.gui
    .alloy
    .set_draw_textures(target, draw, &textures)
    .map_err(|e| throw_str(&ctx, &format!("setDrawTextures: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Per-entry draw range: setDraw addressed to one draw entry, same partial
// merge and mode rule.
fn set_draw_range(ctx: Ctx<'_>, target: u64, draw: u64, update: Object<'_>) -> rquickjs::Result<()> {
  let update = collect_draw_update(&ctx, &update, "setDrawRange")?;
  let st = state(&ctx);
  st.gui.alloy.set_draw_range(target, draw, update).map_err(|e| throw_str(&ctx, &format!("setDrawRange: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// Per-entry buffer swap: the buffer half of setDraw addressed to one draw
// entry (replace-only; the entry's range is kept and rechecked against
// the new buffers). The same transaction as setDrawRange with only
// buffer keys.
fn set_draw_buffers(ctx: Ctx<'_>, target: u64, draw: u64, update: Object<'_>) -> rquickjs::Result<()> {
  let buffers = collect_buffer_update(&ctx, &update, "setDrawBuffers")?;
  let update = alloy::DrawUpdate { buffers, ..Default::default() };
  let st = state(&ctx);
  st.gui.alloy.set_draw_range(target, draw, update).map_err(|e| throw_str(&ctx, &format!("setDrawBuffers: {e}")))?;
  st.gui.platform.request_frame();
  Ok(())
}

// The explicit render verb for manual targets (render: "manual"); alloy
// validates the mode and queues the pass in call order.
fn render_target(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  let st = state(&ctx);
  st.gui.alloy.render_target(id).map_err(|e| throw_str(&ctx, &format!("renderTarget: {e}")))?;
  // New target output changes the screen without any tree mutation.
  st.gui.platform.request_frame();
  Ok(())
}

// The GPU-side seed/history write into a manual target; alloy validates
// ids, sizes, and the mode, and queues the copy in call order.
fn copy_texture(ctx: Ctx<'_>, src: u64, dst: u64) -> rquickjs::Result<()> {
  let st = state(&ctx);
  st.gui.alloy.copy_texture(src, dst).map_err(|e| throw_str(&ctx, &format!("copyTexture: {e}")))?;
  // New target output changes the screen without any tree mutation.
  st.gui.platform.request_frame();
  Ok(())
}

fn destroy_texture(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  let st = state(&ctx);
  // A runtime-owned id (snapshot boundary, camera, video) is released by
  // its owner: the boundary's unmount, the session's close.
  if st.gui.alloy.is_borrowed(id) {
    return Err(rquickjs::Exception::throw_message(
      &ctx,
      &format!(
        "Texture {id} is owned by the runtime (a snapshot boundary, camera or video) and is released by its owner"
      ),
    ));
  }
  // A depth texture id is the target's storage, reclaimed with it.
  if let Some(owner) = st.gui.alloy.depth_owner(id) {
    return Err(throw_str(
      &ctx,
      &format!("destroyTexture: texture {id} is the depth texture of target {owner} and dies with it"),
    ));
  }
  st.created.borrow_mut().remove(&id);
  st.gui.alloy.destroy_texture(id);
  // Destruction is deferred to the paint loop's reclamation sweep, which
  // only runs when a frame is produced - request one so a destroy on an
  // otherwise idle app is not stranded.
  st.gui.platform.request_frame();
  Ok(())
}

/// Queue a node capture and return a promise settled from `tick` once alloy
/// renders it on a later paint pass. The completion callback (run on the UI
/// thread with no `Ctx`) only stashes the outcome and the promise sides into
/// `capture_settle`; `tick` does the JS-facing settle (see CaptureSettle).
fn capture_snapshot<'js>(ctx: Ctx<'js>, node_id: u64) -> rquickjs::Result<Promise<'js>> {
  let st = state(&ctx);
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let resolve = Persistent::save(&ctx, resolve);
  let reject = Persistent::save(&ctx, reject);
  // The callback holds an Rc to our state so it can enqueue the outcome. Alloy
  // owns the callback until the capture is serviced, keeping the state alive
  // while the request is in flight (no cycle: alloy is not owned by the state).
  let inner = st.clone();
  st.gui.alloy.request_capture(
    node_id,
    Box::new(move |result| {
      inner.capture_settle.borrow_mut().push(CaptureSettle { result, resolve, reject });
    }),
  );
  // The capture is serviced during a paint; make sure one happens.
  st.gui.platform.request_frame();
  Ok(promise)
}

/// Read back any registered texture's current RGBA8 pixels (tightly packed,
/// top-to-bottom) as `{ width, height, data }`. Synchronous: the texture was
/// already rendered on this thread's GL context at creation time, so there is
/// nothing to wait for.
fn read_texture<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Object<'js>> {
  let (width, height, pixels) =
    state(&ctx).gui.alloy.read_texture_by_id(id).map_err(|e| throw_str(&ctx, &format!("readTexture: {e}")))?;
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

/// Per-frame hook (see `frame::advance`). Drains the capture outcomes the
/// completion callbacks enqueued during the last paint and settles each
/// promise, tracking the new texture id for reload cleanup.
pub(crate) fn tick(ctx: &Ctx<'_>) {
  let Some(state) = ctx.userdata::<TextureState>() else {
    return;
  };
  let settles = std::mem::take(&mut *state.0.capture_settle.borrow_mut());
  for CaptureSettle { result, resolve, reject } in settles {
    match result {
      Ok(CaptureInfo { pixels, width, height }) => {
        // Plain bytes, no registry entry: nothing to track for reload cleanup.
        let settle = || -> rquickjs::Result<()> {
          let obj = Object::new(ctx.clone())?;
          obj.set("width", width)?;
          obj.set("height", height)?;
          obj.set("data", TypedArray::new(ctx.clone(), pixels)?)?;
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
