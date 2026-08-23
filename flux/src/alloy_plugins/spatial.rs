//! JS bindings for the spatial core (`alloy::spatial`): node create/move/
//! destroy, draw-sink binding and the flush. A thin marshalling layer - the
//! transform is one Float32Array of 10 (position xyz, quaternion xyzw,
//! scale xyz) so a hot-path write is one argument, and node ids are plain
//! numbers (generation-tagged, never reused).

use std::rc::Rc;
use std::sync::Arc;

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Function, JsLifetime, Object, TypedArray, Value};

use super::AlloyContext;
use crate::plugins::marshal::OptArg;
use alloy::rendertree::PlatformContext;
use alloy::spatial::{DrawSink, Projection, Shape, SharedSlotSink};

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

struct SpatialInner {
  atx: AlloyContext,
  platform: Arc<PlatformContext>,
}

#[derive(Clone, JsLifetime)]
struct SpatialState(#[qjs(skip_trace)] Rc<SpatialInner>);

pub fn store_state(ctx: &Ctx<'_>, atx: AlloyContext, platform: Arc<PlatformContext>) {
  ctx.store_userdata(SpatialState(Rc::new(SpatialInner { atx, platform }))).expect("store spatial state");
}

fn state(ctx: &Ctx<'_>) -> Rc<SpatialInner> {
  ctx.userdata::<SpatialState>().expect("spatial state userdata").0.clone()
}

/// The 10 floats of a transform argument: position, quaternion, scale.
fn transform(ctx: &Ctx<'_>, data: &TypedArray<'_, f32>, api: &str) -> rquickjs::Result<([f32; 3], [f32; 4], [f32; 3])> {
  let raw = data.as_raw().ok_or_else(|| throw_str(ctx, &format!("{api}: detached buffer")))?;
  if raw.len != 10 * 4 {
    return Err(throw_str(
      ctx,
      &format!("{api}: transform must be a Float32Array of 10 (position, quaternion, scale)"),
    ));
  }
  let f = unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr() as *const f32, 10) };
  Ok(([f[0], f[1], f[2]], [f[3], f[4], f[5], f[6]], [f[7], f[8], f[9]]))
}

pub struct SpatialModule;

impl ModuleDef for SpatialModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createNode")?;
    decl.declare("destroyNode")?;
    decl.declare("setParent")?;
    decl.declare("setTransform")?;
    decl.declare("setVisible")?;
    decl.declare("bindDraw")?;
    decl.declare("unbindDraw")?;
    decl.declare("setDrawCount")?;
    decl.declare("worldMatrix")?;
    decl.declare("shown")?;
    decl.declare("flush")?;
    decl.declare("setBounds")?;
    decl.declare("createShape")?;
    decl.declare("destroyShape")?;
    decl.declare("setShape")?;
    decl.declare("raycast")?;
    decl.declare("bindDirectionSlot")?;
    decl.declare("unbindSlot")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("createNode", Function::new(ctx.clone(), create_node)?)?;
    exports.export("destroyNode", Function::new(ctx.clone(), destroy_node)?)?;
    exports.export("setParent", Function::new(ctx.clone(), set_parent)?)?;
    exports.export("setTransform", Function::new(ctx.clone(), set_transform)?)?;
    exports.export("setVisible", Function::new(ctx.clone(), set_visible)?)?;
    exports.export("bindDraw", Function::new(ctx.clone(), bind_draw)?)?;
    exports.export("unbindDraw", Function::new(ctx.clone(), unbind_draw)?)?;
    exports.export("setDrawCount", Function::new(ctx.clone(), set_draw_count)?)?;
    exports.export("worldMatrix", Function::new(ctx.clone(), world_matrix)?)?;
    exports.export("shown", Function::new(ctx.clone(), shown)?)?;
    exports.export("flush", Function::new(ctx.clone(), flush)?)?;
    exports.export("setBounds", Function::new(ctx.clone(), set_bounds)?)?;
    exports.export("createShape", Function::new(ctx.clone(), create_shape)?)?;
    exports.export("destroyShape", Function::new(ctx.clone(), destroy_shape)?)?;
    exports.export("setShape", Function::new(ctx.clone(), set_shape)?)?;
    exports.export("raycast", Function::new(ctx.clone(), raycast)?)?;
    exports.export("bindDirectionSlot", Function::new(ctx.clone(), bind_direction_slot)?)?;
    exports.export("unbindSlot", Function::new(ctx.clone(), unbind_slot)?)?;
    Ok(())
  }
}

fn create_node(ctx: Ctx<'_>, data: TypedArray<'_, f32>, visible: bool) -> rquickjs::Result<u64> {
  let (p, q, s) = transform(&ctx, &data, "createNode")?;
  Ok(state(&ctx).atx.spatial().create(p, q, s, visible))
}

fn destroy_node(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial().destroy(id).map_err(|e| throw_str(&ctx, &format!("destroyNode: {e}")))
}

fn set_parent(ctx: Ctx<'_>, id: u64, parent: OptArg<u64>) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial().set_parent(id, parent.0).map_err(|e| throw_str(&ctx, &format!("setParent: {e}")))
}

fn set_transform(ctx: Ctx<'_>, id: u64, data: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let (p, q, s) = transform(&ctx, &data, "setTransform")?;
  state(&ctx).atx.spatial().set_transform(id, p, q, s).map_err(|e| throw_str(&ctx, &format!("setTransform: {e}")))
}

fn set_visible(ctx: Ctx<'_>, id: u64, visible: bool) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial().set_visible(id, visible).map_err(|e| throw_str(&ctx, &format!("setVisible: {e}")))
}

fn bind_draw(ctx: Ctx<'_>, id: u64, target: u64, draw: u64, normal: bool, count: u32) -> rquickjs::Result<()> {
  state(&ctx)
    .atx
    .spatial_bind(id, Some(DrawSink { target, draw, normal, count }))
    .map_err(|e| throw_str(&ctx, &format!("bindDraw: {e}")))
}

fn unbind_draw(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial_bind(id, None).map_err(|e| throw_str(&ctx, &format!("unbindDraw: {e}")))
}

fn set_draw_count(ctx: Ctx<'_>, id: u64, count: u32) -> rquickjs::Result<()> {
  let st = state(&ctx);
  let wrote = st.atx.spatial_set_count(id, count).map_err(|e| throw_str(&ctx, &format!("setDrawCount: {e}")))?;
  if wrote {
    st.platform.request_frame();
  }
  Ok(())
}

/// Fill `out` (a Float32Array of 16) with the node's current world matrix.
fn world_matrix(ctx: Ctx<'_>, id: u64, out: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let world = state(&ctx).atx.spatial().world(id).map_err(|e| throw_str(&ctx, &format!("worldMatrix: {e}")))?;
  let raw = out.as_raw().ok_or_else(|| throw_str(&ctx, "worldMatrix: detached buffer"))?;
  if raw.len != 16 * 4 {
    return Err(throw_str(&ctx, "worldMatrix: out must be a Float32Array of 16"));
  }
  let dst = unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr() as *mut f32, 16) };
  dst.copy_from_slice(&world);
  Ok(())
}

fn shown(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<bool> {
  state(&ctx).atx.spatial().shown(id).map_err(|e| throw_str(&ctx, &format!("shown: {e}")))
}

fn flush(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let st = state(&ctx);
  if st.atx.spatial_flush() {
    st.platform.request_frame();
  }
  Ok(())
}

/// Read a Float32Array as a slice (valid for the call).
fn floats<'a, 'js>(ctx: &Ctx<'_>, data: &'a TypedArray<'js, f32>, api: &str) -> rquickjs::Result<&'a [f32]> {
  let raw = data.as_raw().ok_or_else(|| throw_str(ctx, &format!("{api}: detached buffer")))?;
  Ok(unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr() as *const f32, raw.len / 4) })
}

/// Set (null clears) the node's local tight box [minX, minY, minZ, maxX,
/// maxY, maxZ]; with one the node is in the picking index.
fn set_bounds(ctx: Ctx<'_>, id: u64, bounds: OptArg<TypedArray<'_, f32>>) -> rquickjs::Result<()> {
  let b = match &bounds.0 {
    Some(data) => {
      let f = floats(&ctx, data, "setBounds")?;
      if f.len() != 6 {
        return Err(throw_str(&ctx, "setBounds: bounds must be a Float32Array of 6 (min xyz, max xyz)"));
      }
      Some([f[0], f[1], f[2], f[3], f[4], f[5]])
    }
    None => None,
  };
  state(&ctx).atx.spatial().set_bounds(id, b).map_err(|e| throw_str(&ctx, &format!("setBounds: {e}")))
}

/// Triangle data for the narrowphase: positions are read from an
/// interleaved vertex array (`stride` floats per vertex, xyz at
/// `posOffset`, uv at `uvOffset` or -1 for none); `indices` is a
/// Uint16Array or Uint32Array triangle list. Returns the shape id.
fn create_shape<'js>(
  ctx: Ctx<'js>,
  vertices: TypedArray<'js, f32>,
  stride: u32,
  pos_offset: u32,
  uv_offset: i32,
  indices: Value<'js>,
) -> rquickjs::Result<u64> {
  let v = floats(&ctx, &vertices, "createShape")?;
  let stride = stride as usize;
  if stride < 3 || pos_offset as usize + 3 > stride || (uv_offset >= 0 && uv_offset as usize + 2 > stride) {
    return Err(throw_str(&ctx, "createShape: offsets do not fit the stride"));
  }
  let count = if stride == 0 { 0 } else { v.len() / stride };
  let mut positions = Vec::with_capacity(count * 3);
  let mut uvs = if uv_offset >= 0 { Some(Vec::with_capacity(count * 2)) } else { None };
  for i in 0..count {
    let base = i * stride + pos_offset as usize;
    positions.extend_from_slice(&v[base..base + 3]);
    if let Some(uvs) = uvs.as_mut() {
      let base = i * stride + uv_offset as usize;
      uvs.extend_from_slice(&v[base..base + 2]);
    }
  }
  let indices: Vec<u32> =
    if let Some(u16s) = indices.as_object().and_then(|o| TypedArray::<u16>::from_object(o.clone()).ok()) {
      let raw = u16s.as_raw().ok_or_else(|| throw_str(&ctx, "createShape: detached buffer"))?;
      unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr() as *const u16, raw.len / 2) }
        .iter()
        .map(|&i| i as u32)
        .collect()
    } else if let Some(u32s) = indices.as_object().and_then(|o| TypedArray::<u32>::from_object(o.clone()).ok()) {
      let raw = u32s.as_raw().ok_or_else(|| throw_str(&ctx, "createShape: detached buffer"))?;
      unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr() as *const u32, raw.len / 4) }.to_vec()
    } else {
      return Err(throw_str(&ctx, "createShape: indices must be a Uint16Array or Uint32Array"));
    };
  state(&ctx)
    .atx
    .spatial()
    .create_shape(Shape { positions, uvs, indices })
    .map_err(|e| throw_str(&ctx, &format!("createShape: {e}")))
}

fn destroy_shape(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial().destroy_shape(id).map_err(|e| throw_str(&ctx, &format!("destroyShape: {e}")))
}

fn set_shape(ctx: Ctx<'_>, id: u64, shape: OptArg<u64>) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial().set_shape(id, shape.0).map_err(|e| throw_str(&ctx, &format!("setShape: {e}")))
}

/// Every shown node with bounds the ray strikes, nearest first, as
/// `{ node, distance, point, normal?, face?, uv? }` objects.
fn raycast<'js>(ctx: Ctx<'js>, origin: TypedArray<'js, f32>, direction: TypedArray<'js, f32>) -> rquickjs::Result<Array<'js>> {
  let o = floats(&ctx, &origin, "raycast")?;
  let d = floats(&ctx, &direction, "raycast")?;
  if o.len() != 3 || d.len() != 3 {
    return Err(throw_str(&ctx, "raycast: origin and direction must be Float32Arrays of 3"));
  }
  let hits = state(&ctx).atx.spatial().raycast([o[0], o[1], o[2]], [d[0], d[1], d[2]]);
  let arr = Array::new(ctx.clone())?;
  for (i, h) in hits.iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("node", h.node)?;
    obj.set("distance", h.distance as f64)?;
    obj.set("point", vec![h.point[0] as f64, h.point[1] as f64, h.point[2] as f64])?;
    if let Some(n) = h.normal {
      obj.set("normal", vec![n[0] as f64, n[1] as f64, n[2] as f64])?;
    }
    if let Some(f) = h.face {
      obj.set("face", f)?;
    }
    if let Some(uv) = h.uv {
      obj.set("uv", vec![uv[0] as f64, uv[1] as f64])?;
    }
    arr.set(i, obj)?;
  }
  Ok(arr)
}

/// Bind the node's shared-slot sink with the direction projection: slot
/// `index` of the `len`-float shared array param `name` on `target`
/// follows the world direction of the LOCAL vector (a Float32Array of 3).
fn bind_direction_slot(
  ctx: Ctx<'_>,
  id: u64,
  target: u64,
  name: String,
  len: u32,
  index: u32,
  vector: TypedArray<'_, f32>,
) -> rquickjs::Result<()> {
  let v = floats(&ctx, &vector, "bindDirectionSlot")?;
  if v.len() != 3 {
    return Err(throw_str(&ctx, "bindDirectionSlot: vector must be a Float32Array of 3"));
  }
  let sink = SharedSlotSink { target, name, len, index, projection: Projection::Direction([v[0], v[1], v[2]]) };
  state(&ctx).atx.spatial_bind_slot(id, Some(sink)).map_err(|e| throw_str(&ctx, &format!("bindDirectionSlot: {e}")))
}

fn unbind_slot(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  state(&ctx).atx.spatial_bind_slot(id, None).map_err(|e| throw_str(&ctx, &format!("unbindSlot: {e}")))
}
