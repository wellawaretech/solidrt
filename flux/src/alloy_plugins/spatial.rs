//! JS bindings for the spatial core (`alloy::spatial`): node create/move/
//! destroy, draw-sink binding and the flush. A thin marshalling layer - the
//! transform is one Float32Array of 10 (position xyz, quaternion xyzw,
//! scale xyz) so a hot-path write is one argument, and node ids are plain
//! numbers (generation-tagged, never reused).


use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Function, Object, TypedArray, Value};

use crate::alloy_plugins::properties::transition::decode_spec;
use crate::alloy_plugins::value::PropValue;
use crate::plugins::marshal::OptArg;
use alloy::spatial::{
  ChannelInterpolation, ChannelPath, ClipChannel, ClipEvent, Component, DrawSink, InstanceProjection,
  InstanceRecordSink, NodeTransitionConfig, PlayerUpdate, Projection, Shape, SharedSlotSink, TextureSlotSink,
};

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

// The spatial bindings keep no state of their own: every call forwards to
// the arena in the shared alloy context (`super::gui`).

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
    decl.declare("setTransition")?;
    decl.declare("writeTransform")?;
    decl.declare("setVisible")?;
    decl.declare("bindDraw")?;
    decl.declare("unbindDraw")?;
    decl.declare("setDrawCount")?;
    decl.declare("worldMatrix")?;
    decl.declare("shown")?;
    decl.declare("flush")?;
    decl.declare("setBounds")?;
    decl.declare("setFrustum")?;
    decl.declare("setCull")?;
    decl.declare("setCullBounds")?;
    decl.declare("setCullGroup")?;
    decl.declare("createShape")?;
    decl.declare("destroyShape")?;
    decl.declare("setShape")?;
    decl.declare("raycast")?;
    decl.declare("overlap")?;
    decl.declare("bindDirectionSlot")?;
    decl.declare("bindPositionSlot")?;
    decl.declare("unbindSlot")?;
    decl.declare("bindTextureSlot")?;
    decl.declare("unbindTextureSlot")?;
    decl.declare("createClip")?;
    decl.declare("destroyClip")?;
    decl.declare("createPlayer")?;
    decl.declare("setPlayer")?;
    decl.declare("destroyPlayer")?;
    decl.declare("readTransform")?;
    decl.declare("bindPoseRecord")?;
    decl.declare("unbindRecord")?;
    decl.declare("retargetRecords")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("createNode", Function::new(ctx.clone(), create_node)?)?;
    exports.export("destroyNode", Function::new(ctx.clone(), destroy_node)?)?;
    exports.export("setParent", Function::new(ctx.clone(), set_parent)?)?;
    exports.export("setTransform", Function::new(ctx.clone(), set_transform)?)?;
    exports.export("setTransition", Function::new(ctx.clone(), set_transition)?)?;
    exports.export("writeTransform", Function::new(ctx.clone(), write_transform)?)?;
    exports.export("setVisible", Function::new(ctx.clone(), set_visible)?)?;
    exports.export("bindDraw", Function::new(ctx.clone(), bind_draw)?)?;
    exports.export("unbindDraw", Function::new(ctx.clone(), unbind_draw)?)?;
    exports.export("setDrawCount", Function::new(ctx.clone(), set_draw_count)?)?;
    exports.export("worldMatrix", Function::new(ctx.clone(), world_matrix)?)?;
    exports.export("shown", Function::new(ctx.clone(), shown)?)?;
    exports.export("flush", Function::new(ctx.clone(), flush)?)?;
    exports.export("setBounds", Function::new(ctx.clone(), set_bounds)?)?;
    exports.export("setFrustum", Function::new(ctx.clone(), set_frustum)?)?;
    exports.export("setCull", Function::new(ctx.clone(), set_cull)?)?;
    exports.export("setCullBounds", Function::new(ctx.clone(), set_cull_bounds)?)?;
    exports.export("setCullGroup", Function::new(ctx.clone(), set_cull_group)?)?;
    exports.export("createShape", Function::new(ctx.clone(), create_shape)?)?;
    exports.export("destroyShape", Function::new(ctx.clone(), destroy_shape)?)?;
    exports.export("setShape", Function::new(ctx.clone(), set_shape)?)?;
    exports.export("raycast", Function::new(ctx.clone(), raycast)?)?;
    exports.export("overlap", Function::new(ctx.clone(), overlap)?)?;
    exports.export("bindDirectionSlot", Function::new(ctx.clone(), bind_direction_slot)?)?;
    exports.export("bindPositionSlot", Function::new(ctx.clone(), bind_position_slot)?)?;
    exports.export("unbindSlot", Function::new(ctx.clone(), unbind_slot)?)?;
    exports.export("bindTextureSlot", Function::new(ctx.clone(), bind_texture_slot)?)?;
    exports.export("unbindTextureSlot", Function::new(ctx.clone(), unbind_texture_slot)?)?;
    exports.export("createClip", Function::new(ctx.clone(), create_clip)?)?;
    exports.export("destroyClip", Function::new(ctx.clone(), destroy_clip)?)?;
    exports.export("createPlayer", Function::new(ctx.clone(), create_player)?)?;
    exports.export("setPlayer", Function::new(ctx.clone(), set_player)?)?;
    exports.export("destroyPlayer", Function::new(ctx.clone(), destroy_player)?)?;
    exports.export("readTransform", Function::new(ctx.clone(), read_transform)?)?;
    exports.export("bindPoseRecord", Function::new(ctx.clone(), bind_pose_record)?)?;
    exports.export("unbindRecord", Function::new(ctx.clone(), unbind_record)?)?;
    exports.export("retargetRecords", Function::new(ctx.clone(), retarget_records)?)?;
    Ok(())
  }
}

fn create_node(ctx: Ctx<'_>, data: TypedArray<'_, f32>, visible: bool) -> rquickjs::Result<u64> {
  let (p, q, s) = transform(&ctx, &data, "createNode")?;
  Ok(super::gui(&ctx).alloy.spatial().create(p, q, s, visible))
}

fn destroy_node(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy(id).map_err(|e| throw_str(&ctx, &format!("destroyNode: {e}")))
}

fn set_parent(ctx: Ctx<'_>, id: u64, parent: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_parent(id, parent.0).map_err(|e| throw_str(&ctx, &format!("setParent: {e}")))
}

fn set_transform(ctx: Ctx<'_>, id: u64, data: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let (p, q, s) = transform(&ctx, &data, "setTransform")?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_transform(id, p, q, s)
    .map_err(|e| throw_str(&ctx, &format!("setTransform: {e}")))
}

/// The node transition declaration: an object keyed by transform component
/// (position, rotation, scale, plus `all` as a catch-all) whose values
/// speak the element transition vocabulary minus the lifecycle
/// conveniences, or a bare shorthand string as the `all` catch-all.
fn decode_node_transition(value: &PropValue) -> Result<NodeTransitionConfig, String> {
  if value.as_str().is_some() {
    return Ok(NodeTransitionConfig { all: Some(decode_spec("transition", value)?), ..Default::default() });
  }
  let entries = value.as_map().ok_or_else(|| {
    "transition must be a shorthand string or an object keyed by component (position, rotation, scale, all)"
      .to_string()
  })?;
  let mut config = NodeTransitionConfig::default();
  for (key, entry) in entries {
    let spec = Some(decode_spec(&format!("transition.{key}"), entry)?);
    match key.as_str() {
      "position" => config.position = spec,
      "rotation" => config.rotation = spec,
      "scale" => config.scale = spec,
      "all" => config.all = spec,
      other => {
        return Err(format!(
          "transition.{other}: '{other}' is not a transform component (expected position, rotation, scale or all)"
        ))
      }
    }
  }
  Ok(config)
}

/// Declare (or with null clear) the node's transition config; with one set,
/// writeTransform animates instead of snapping. Clearing cancels running
/// tracks in place (no settled events) and later writes snap.
fn set_transition<'js>(ctx: Ctx<'js>, id: u64, value: Value<'js>) -> rquickjs::Result<()> {
  let config = if value.is_null() || value.is_undefined() {
    None
  } else {
    let pv = super::tree::to_prop_value(&value)?;
    Some(decode_node_transition(&pv).map_err(|e| throw_str(&ctx, &format!("setTransition: {e}")))?)
  };
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_node_transition(id, config)
    .map_err(|e| throw_str(&ctx, &format!("setTransition: {e}")))
}

/// Replace the local transform through the transition declaration: declared
/// components animate toward the written value, undeclared ones snap;
/// without a declaration this is setTransform. A started or retargeted
/// track (or a snap that moved the node) requests a frame.
fn write_transform(ctx: Ctx<'_>, id: u64, data: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let (p, q, s) = transform(&ctx, &data, "writeTransform")?;
  let st = super::gui(&ctx);
  let changed =
    st.alloy.spatial().write_transform(id, p, q, s).map_err(|e| throw_str(&ctx, &format!("writeTransform: {e}")))?;
  if changed {
    st.platform.request_frame();
  }
  Ok(())
}

fn set_visible(ctx: Ctx<'_>, id: u64, visible: bool) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_visible(id, visible).map_err(|e| throw_str(&ctx, &format!("setVisible: {e}")))
}

fn bind_draw(ctx: Ctx<'_>, id: u64, target: u64, draw: u64, normal: bool, count: u32) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial_bind(id, DrawSink { target, draw, normal, count })
    .map_err(|e| throw_str(&ctx, &format!("bindDraw: {e}")))
}

/// Remove the node's draw sink on `target`, or every draw sink without one.
fn unbind_draw(ctx: Ctx<'_>, id: u64, target: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial_unbind(id, target.0).map_err(|e| throw_str(&ctx, &format!("unbindDraw: {e}")))
}

fn set_draw_count(ctx: Ctx<'_>, id: u64, count: u32) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  let wrote = st.alloy.spatial_set_count(id, count).map_err(|e| throw_str(&ctx, &format!("setDrawCount: {e}")))?;
  if wrote {
    st.platform.request_frame();
  }
  Ok(())
}

/// Fill `out` (a Float32Array of 16) with the node's current world matrix.
fn world_matrix(ctx: Ctx<'_>, id: u64, out: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let world = super::gui(&ctx).alloy.spatial().world(id).map_err(|e| throw_str(&ctx, &format!("worldMatrix: {e}")))?;
  let raw = out.as_raw().ok_or_else(|| throw_str(&ctx, "worldMatrix: detached buffer"))?;
  if raw.len != 16 * 4 {
    return Err(throw_str(&ctx, "worldMatrix: out must be a Float32Array of 16"));
  }
  let dst = unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr() as *mut f32, 16) };
  dst.copy_from_slice(&world);
  Ok(())
}

fn shown(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<bool> {
  super::gui(&ctx).alloy.spatial().shown(id).map_err(|e| throw_str(&ctx, &format!("shown: {e}")))
}

fn flush(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  if st.alloy.spatial_flush() {
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
  super::gui(&ctx).alloy.spatial().set_bounds(id, b).map_err(|e| throw_str(&ctx, &format!("setBounds: {e}")))
}

/// A six-float box argument (min xyz, max xyz), or None.
fn box_arg(ctx: &Ctx<'_>, bounds: &OptArg<TypedArray<'_, f32>>, api: &str) -> rquickjs::Result<Option<[f32; 6]>> {
  match &bounds.0 {
    Some(data) => {
      let f = floats(ctx, data, api)?;
      if f.len() != 6 {
        return Err(throw_str(ctx, &format!("{api}: bounds must be a Float32Array of 6 (min xyz, max xyz)")));
      }
      Ok(Some([f[0], f[1], f[2], f[3], f[4], f[5]]))
    }
    None => Ok(None),
  }
}

/// The clip volume gating a target's draw sinks: its view-projection as a
/// Float32Array of 16 (column-major), or null to lift it.
fn set_frustum(ctx: Ctx<'_>, target: u64, view_proj: OptArg<TypedArray<'_, f32>>) -> rquickjs::Result<()> {
  let m = match &view_proj.0 {
    Some(data) => {
      let f = floats(&ctx, data, "setFrustum")?;
      if f.len() != 16 {
        return Err(throw_str(&ctx, "setFrustum: viewProj must be a Float32Array of 16"));
      }
      let mut m = [0.0f32; 16];
      m.copy_from_slice(f);
      Some(m)
    }
    None => None,
  };
  super::gui(&ctx).alloy.spatial().set_frustum(target, m);
  Ok(())
}

fn set_cull(ctx: Ctx<'_>, id: u64, enabled: bool, margin: f64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_cull(id, enabled, margin as f32).map_err(|e| throw_str(&ctx, &format!("setCull: {e}")))
}

fn set_cull_bounds(ctx: Ctx<'_>, id: u64, bounds: OptArg<TypedArray<'_, f32>>) -> rquickjs::Result<()> {
  let b = box_arg(&ctx, &bounds, "setCullBounds")?;
  super::gui(&ctx).alloy.spatial().set_cull_bounds(id, b).map_err(|e| throw_str(&ctx, &format!("setCullBounds: {e}")))
}

fn set_cull_group(ctx: Ctx<'_>, id: u64, members: Vec<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_cull_group(id, &members).map_err(|e| throw_str(&ctx, &format!("setCullGroup: {e}")))
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
  super::gui(&ctx)
    .alloy
    .spatial()
    .create_shape(Shape { positions, uvs, indices })
    .map_err(|e| throw_str(&ctx, &format!("createShape: {e}")))
}

fn destroy_shape(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_shape(id).map_err(|e| throw_str(&ctx, &format!("destroyShape: {e}")))
}

fn set_shape(ctx: Ctx<'_>, id: u64, shape: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_shape(id, shape.0).map_err(|e| throw_str(&ctx, &format!("setShape: {e}")))
}

/// Every shown node with bounds the ray strikes, nearest first, as
/// `{ node, distance, point, normal?, face?, uv? }` objects.
fn raycast<'js>(ctx: Ctx<'js>, origin: TypedArray<'js, f32>, direction: TypedArray<'js, f32>) -> rquickjs::Result<Array<'js>> {
  let o = floats(&ctx, &origin, "raycast")?;
  let d = floats(&ctx, &direction, "raycast")?;
  if o.len() != 3 || d.len() != 3 {
    return Err(throw_str(&ctx, "raycast: origin and direction must be Float32Arrays of 3"));
  }
  let hits = super::gui(&ctx).alloy.spatial().raycast([o[0], o[1], o[2]], [d[0], d[1], d[2]]);
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

/// Every shown node with bounds whose transformed local box overlaps the
/// world-axis box, as an array of node ids (unordered).
fn overlap<'js>(ctx: Ctx<'js>, bounds: TypedArray<'js, f32>) -> rquickjs::Result<Array<'js>> {
  let b = floats(&ctx, &bounds, "overlap")?;
  if b.len() != 6 {
    return Err(throw_str(&ctx, "overlap: bounds must be a Float32Array of 6 (minX..maxZ)"));
  }
  let nodes = super::gui(&ctx).alloy.spatial().overlap([b[0], b[1], b[2], b[3], b[4], b[5]]);
  let arr = Array::new(ctx.clone())?;
  for (i, id) in nodes.iter().enumerate() {
    arr.set(i, *id)?;
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
  super::gui(&ctx).alloy.spatial_bind_slot(id, sink).map_err(|e| throw_str(&ctx, &format!("bindDirectionSlot: {e}")))
}

/// Bind the node's shared-slot sink with the position projection: slot
/// `index` of the `len`-float shared array param `name` on `target`
/// follows the node's world position.
fn bind_position_slot(ctx: Ctx<'_>, id: u64, target: u64, name: String, len: u32, index: u32) -> rquickjs::Result<()> {
  let sink = SharedSlotSink { target, name, len, index, projection: Projection::Position };
  super::gui(&ctx).alloy.spatial_bind_slot(id, sink).map_err(|e| throw_str(&ctx, &format!("bindPositionSlot: {e}")))
}

/// Remove the node's slot sink on `target`, or every slot sink without one.
fn unbind_slot(ctx: Ctx<'_>, id: u64, target: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial_unbind_slot(id, target.0).map_err(|e| throw_str(&ctx, &format!("unbindSlot: {e}")))
}

/// Bind the node's texture slot: the flush writes the node's world matrix,
/// post-multiplied by `post` (a Float32Array of 16, column-major), as the
/// 16 floats of row `row` of the rgba32f texture - one whole-palette
/// upload per texture per flush. With `anchor` (an ancestor node shared by
/// every slot on the texture) rows are anchor-local:
/// inverse(anchorWorld) * world * post.
fn bind_texture_slot(
  ctx: Ctx<'_>,
  id: u64,
  texture: u64,
  row: u32,
  post: TypedArray<'_, f32>,
  anchor: OptArg<u64>,
) -> rquickjs::Result<()> {
  let p = floats(&ctx, &post, "bindTextureSlot")?;
  if p.len() != 16 {
    return Err(throw_str(&ctx, "bindTextureSlot: post must be a Float32Array of 16 (a column-major mat4)"));
  }
  let mut m = [0.0f32; 16];
  m.copy_from_slice(p);
  super::gui(&ctx)
    .alloy
    .spatial_bind_texture_slot(id, TextureSlotSink { texture, row, post: m }, anchor.0)
    .map_err(|e| throw_str(&ctx, &format!("bindTextureSlot: {e}")))
}

/// Remove the node's texture slot on `texture`, or every texture slot
/// without one; abandoned rows keep their last value.
fn unbind_texture_slot(ctx: Ctx<'_>, id: u64, texture: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial_unbind_texture_slot(id, texture.0)
    .map_err(|e| throw_str(&ctx, &format!("unbindTextureSlot: {e}")))
}

// Meta words per channel in createClip's packed layout.
const CLIP_META_WORDS: usize = 4;

/// Register a baked clip: `meta` is [targetSlot, path (0 position,
/// 1 rotation, 2 scale), interpolation (0 step, 1 linear, 2 cubic),
/// keyCount] per channel; `times` and `values` are every channel's key
/// arrays concatenated in meta order. One crossing per clip.
fn create_clip<'js>(
  ctx: Ctx<'js>,
  duration: f64,
  meta: TypedArray<'js, u32>,
  times: TypedArray<'js, f32>,
  values: TypedArray<'js, f32>,
) -> rquickjs::Result<u64> {
  let meta_raw = meta.as_raw().ok_or_else(|| throw_str(&ctx, "createClip: detached buffer"))?;
  let meta: &[u32] = unsafe { std::slice::from_raw_parts(meta_raw.ptr.as_ptr() as *const u32, meta_raw.len / 4) };
  let times = floats(&ctx, &times, "createClip")?;
  let values = floats(&ctx, &values, "createClip")?;
  if meta.len() % CLIP_META_WORDS != 0 {
    return Err(throw_str(&ctx, "createClip: meta must be 4 words per channel"));
  }
  let mut channels = Vec::with_capacity(meta.len() / CLIP_META_WORDS);
  let mut t_at = 0usize;
  let mut v_at = 0usize;
  for (i, entry) in meta.chunks(CLIP_META_WORDS).enumerate() {
    let path = match entry[1] {
      0 => ChannelPath::Position,
      1 => ChannelPath::Rotation,
      2 => ChannelPath::Scale,
      other => return Err(throw_str(&ctx, &format!("createClip: channel {i} path {other} is not 0, 1 or 2"))),
    };
    let interpolation = match entry[2] {
      0 => ChannelInterpolation::Step,
      1 => ChannelInterpolation::Linear,
      2 => ChannelInterpolation::Cubic,
      other => {
        return Err(throw_str(&ctx, &format!("createClip: channel {i} interpolation {other} is not 0, 1 or 2")))
      }
    };
    let keys = entry[3] as usize;
    let elements = if path == ChannelPath::Rotation { 4 } else { 3 };
    let stride = if interpolation == ChannelInterpolation::Cubic { elements * 3 } else { elements };
    let t_end = t_at + keys;
    let v_end = v_at + keys * stride;
    if t_end > times.len() || v_end > values.len() {
      return Err(throw_str(&ctx, &format!("createClip: channel {i} runs past the times/values arrays")));
    }
    channels.push(ClipChannel {
      target_slot: entry[0],
      path,
      interpolation,
      times: times[t_at..t_end].to_vec(),
      values: values[v_at..v_end].to_vec(),
    });
    t_at = t_end;
    v_at = v_end;
  }
  if t_at != times.len() || v_at != values.len() {
    return Err(throw_str(&ctx, "createClip: times/values are longer than meta describes"));
  }
  super::gui(&ctx)
    .alloy
    .spatial()
    .create_clip(duration, channels)
    .map_err(|e| throw_str(&ctx, &format!("createClip: {e}")))
}

fn destroy_clip(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_clip(id).map_err(|e| throw_str(&ctx, &format!("destroyClip: {e}")))
}

/// Start a player: `targets[slot]` is the node each clip channel's target
/// slot animates. Every target must be a live scene node.
fn create_player(
  ctx: Ctx<'_>,
  clip: u64,
  targets: Vec<u64>,
  speed: f64,
  looped: bool,
  weight: f64,
  fade: f64,
) -> rquickjs::Result<u64> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .create_player(clip, targets, speed as f32, looped, weight as f32, fade as f32)
    .map_err(|e| throw_str(&ctx, &format!("createPlayer: {e}")))
}

/// Write the given fields of a player: { weight?, fade?, speed?, time? }.
/// Setting time re-arms a finished player's end report.
fn set_player<'js>(ctx: Ctx<'js>, id: u64, value: Object<'js>) -> rquickjs::Result<()> {
  let update = PlayerUpdate {
    weight: value.get::<_, Option<f64>>("weight")?.map(|v| v as f32),
    fade: value.get::<_, Option<f64>>("fade")?.map(|v| v as f32),
    speed: value.get::<_, Option<f64>>("speed")?.map(|v| v as f32),
    time: value.get::<_, Option<f64>>("time")?,
  };
  super::gui(&ctx).alloy.spatial().set_player(id, update).map_err(|e| throw_str(&ctx, &format!("setPlayer: {e}")))
}

fn destroy_player(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_player(id);
  Ok(())
}

/// Fill `out` (a Float32Array of 10) with the node's current local TRS
/// (position, quaternion, scale) - what the players last wrote, or any
/// later snap. The pose read for root-motion strips and skeleton copies.
fn read_transform(ctx: Ctx<'_>, id: u64, out: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let (p, q, s) =
    super::gui(&ctx).alloy.spatial().transform_of(id).map_err(|e| throw_str(&ctx, &format!("readTransform: {e}")))?;
  let raw = out.as_raw().ok_or_else(|| throw_str(&ctx, "readTransform: detached buffer"))?;
  if raw.len != 10 * 4 {
    return Err(throw_str(&ctx, "readTransform: out must be a Float32Array of 10"));
  }
  let dst = unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr() as *mut f32, 10) };
  dst[0..3].copy_from_slice(&p);
  dst[3..7].copy_from_slice(&q);
  dst[7..10].copy_from_slice(&s);
  Ok(())
}

/// Bind the node's instance-record sink with the 2D pose projection: the
/// flush writes [x, y, angle, sx, sy] to record slot `index` of vertex
/// buffer `buffer`, batched into one buffer write per flush.
fn bind_pose_record(ctx: Ctx<'_>, id: u64, buffer: u64, index: u32) -> rquickjs::Result<()> {
  let sink = InstanceRecordSink { buffer, index, projection: InstanceProjection::Pose2D };
  super::gui(&ctx)
    .alloy
    .spatial_bind_record(id, Some(sink))
    .map_err(|e| throw_str(&ctx, &format!("bindPoseRecord: {e}")))
}

fn unbind_record(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial_bind_record(id, None).map_err(|e| throw_str(&ctx, &format!("unbindRecord: {e}")))
}

/// Move every record sink on buffer `old` to buffer `new` - the growth
/// swap: one call and one bulk republish instead of a rebind per node.
fn retarget_records(ctx: Ctx<'_>, old: u64, new: u64) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial_retarget_records(old, new)
    .map_err(|e| throw_str(&ctx, &format!("retargetRecords: {e}")))
}

/// Stamp the node-transition animation clock (the frame module, once per
/// frame with the app timeline before the frame's JS runs, beside the
/// render tree's stamp). No-op before the GUI is installed.
pub(crate) fn stamp_clock(ctx: &Ctx<'_>, now_ms: f64) {
  if let Some(g) = super::try_gui(ctx) {
    g.alloy.spatial().set_transition_now(now_ms);
  }
}

/// What a frame's node-transition tick produced (see `tick`).
pub(crate) struct SpatialTick {
  /// Tracks still run: the runner's signal to keep requesting frames.
  pub active: bool,
  /// The flush sent sink writes: this frame must paint.
  pub wrote: bool,
}

/// What a frame's clip-player advance produced (see `advance_players`).
pub(crate) struct PlayersTick {
  /// Players can still progress: keep requesting frames.
  pub active: bool,
  /// Node TRS changed: this frame must flush and paint.
  pub wrote: bool,
}

/// Advance the clip players to the stamped clock and write the blended
/// poses into the arena. The frame module calls this BEFORE the frame's JS
/// (right after stamping the clock), so `onFrame` handlers read and can
/// overwrite freshly posed nodes - the post-animation hook - and the draw
/// path's flush publishes the result. Finished/dropped players reach JS
/// as one "spatialClipEnd" engine event each, payload `{ player, reason }`
/// (reason "finished" or "dropped"), emitted here so handlers run in the
/// same frame's turn.
pub(crate) fn advance_players(ctx: &Ctx<'_>) -> PlayersTick {
  let Some(st) = super::try_gui(ctx) else {
    return PlayersTick { active: false, wrote: false };
  };
  let tick = st.alloy.spatial().advance_players();
  let events = st.alloy.spatial().take_clip_events();
  for event in events {
    let obj = Object::new(ctx.clone()).expect("create spatialClipEnd object");
    let (player, reason) = match event {
      ClipEvent::Finished(id) => (id, "finished"),
      ClipEvent::Dropped(id) => (id, "dropped"),
    };
    obj.set("player", player).expect("set player");
    obj.set("reason", reason).expect("set reason");
    crate::emit_event(ctx, "spatialClipEnd", obj);
  }
  PlayersTick { active: tick.active, wrote: tick.wrote }
}

/// Advance the node transitions to the stamped clock and publish what
/// moved: steps every running track (writing node TRS through the arena's
/// ordinary snap path), flushes the arena when anything was written, and
/// emits one "spatialTransitionEnd" engine event per settled track,
/// payload `{ node, component }`. `frame::draw` calls this beside the
/// render tree's transition advance, before the frame's demand gate.
pub(crate) fn tick(ctx: &Ctx<'_>) -> SpatialTick {
  let Some(st) = super::try_gui(ctx) else {
    return SpatialTick { active: false, wrote: false };
  };
  let active = st.alloy.spatial().advance_transitions();
  let settled = st.alloy.spatial().take_settled_transitions();
  // The flush is unconditional: besides transition writes, the queue may
  // hold clip-player poses (advanced before the frame's JS) and whatever
  // that JS wrote without its own microtask flush landing yet. An empty
  // queue is a cheap no-op.
  let wrote = st.alloy.spatial_flush();
  for (node, component) in settled {
    let obj = Object::new(ctx.clone()).expect("create spatialTransitionEnd object");
    obj.set("node", node).expect("set node");
    let name = match component {
      Component::Position => "position",
      Component::Rotation => "rotation",
      Component::Scale => "scale",
    };
    obj.set("component", name).expect("set component");
    crate::emit_event(ctx, "spatialTransitionEnd", obj);
  }
  SpatialTick { active, wrote }
}
