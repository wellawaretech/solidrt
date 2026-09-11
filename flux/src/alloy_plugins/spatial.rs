//! JS bindings for the spatial core (`alloy::spatial`): node create/move/
//! destroy, draw-sink binding and the flush. A thin marshalling layer - the
//! transform is one Float32Array of 10 (position xyz, quaternion xyzw,
//! scale xyz) so a hot-path write is one argument, and node ids are plain
//! numbers (generation-tagged, never reused).

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Function, Object, TypedArray, Value};

use crate::alloy_plugins::properties::transition::{
  decode_node_entry, decode_node_motion, decode_stagger, NodeEntryDecoded,
};
use crate::alloy_plugins::value::PropValue;
use crate::plugins::marshal::OptArg;
use alloy::spatial::{
  ChannelInterpolation, ChannelPath, ClipChannel, ClipEvent, Component, DrawSink, InstanceProjection,
  InstanceRecordSink, MoveOptions, NodeEndpoint, NodeMotion, NodeTransitionConfig, NodeTransitionEntry, PlayerUpdate,
  Projection, QueryFilter, RootMotion, Shape, SharedSlotSink, TextureSlotSink, Volume,
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
    decl.declare("exitNode")?;
    decl.declare("describeNode")?;
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
    decl.declare("updateShape")?;
    decl.declare("destroyShape")?;
    decl.declare("setShape")?;
    decl.declare("setLayers")?;
    decl.declare("raycast")?;
    decl.declare("overlap")?;
    decl.declare("sweep")?;
    decl.declare("moveAndSlide")?;
    decl.declare("bindDirectionSlot")?;
    decl.declare("bindPositionSlot")?;
    decl.declare("unbindSlot")?;
    decl.declare("bindTextureSlot")?;
    decl.declare("unbindTextureSlot")?;
    decl.declare("createClip")?;
    decl.declare("destroyClip")?;
    decl.declare("createPlayer")?;
    decl.declare("setPlayer")?;
    decl.declare("bindRootMotion")?;
    decl.declare("destroyPlayer")?;
    decl.declare("readTransform")?;
    decl.declare("bindPoseRecord")?;
    decl.declare("bindMatrixRecord")?;
    decl.declare("unbindRecord")?;
    decl.declare("retargetRecords")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("createNode", Function::new(ctx.clone(), create_node)?)?;
    exports.export("destroyNode", Function::new(ctx.clone(), destroy_node)?)?;
    exports.export("exitNode", Function::new(ctx.clone(), exit_node)?)?;
    exports.export("describeNode", Function::new(ctx.clone(), describe_node)?)?;
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
    exports.export("updateShape", Function::new(ctx.clone(), update_shape)?)?;
    exports.export("destroyShape", Function::new(ctx.clone(), destroy_shape)?)?;
    exports.export("setShape", Function::new(ctx.clone(), set_shape)?)?;
    exports.export("setLayers", Function::new(ctx.clone(), set_layers)?)?;
    exports.export("raycast", Function::new(ctx.clone(), raycast)?)?;
    exports.export("overlap", Function::new(ctx.clone(), overlap)?)?;
    exports.export("sweep", Function::new(ctx.clone(), sweep)?)?;
    exports.export("moveAndSlide", Function::new(ctx.clone(), move_and_slide)?)?;
    exports.export("bindDirectionSlot", Function::new(ctx.clone(), bind_direction_slot)?)?;
    exports.export("bindPositionSlot", Function::new(ctx.clone(), bind_position_slot)?)?;
    exports.export("unbindSlot", Function::new(ctx.clone(), unbind_slot)?)?;
    exports.export("bindTextureSlot", Function::new(ctx.clone(), bind_texture_slot)?)?;
    exports.export("unbindTextureSlot", Function::new(ctx.clone(), unbind_texture_slot)?)?;
    exports.export("createClip", Function::new(ctx.clone(), create_clip)?)?;
    exports.export("destroyClip", Function::new(ctx.clone(), destroy_clip)?)?;
    exports.export("createPlayer", Function::new(ctx.clone(), create_player)?)?;
    exports.export("setPlayer", Function::new(ctx.clone(), set_player)?)?;
    exports.export("bindRootMotion", Function::new(ctx.clone(), bind_root_motion)?)?;
    exports.export("destroyPlayer", Function::new(ctx.clone(), destroy_player)?)?;
    exports.export("readTransform", Function::new(ctx.clone(), read_transform)?)?;
    exports.export("bindPoseRecord", Function::new(ctx.clone(), bind_pose_record)?)?;
    exports.export("bindMatrixRecord", Function::new(ctx.clone(), bind_matrix_record)?)?;
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

/// Let go of a node through its declaration's exits (alloy `Spatial::exit`):
/// returns whether it is now leaving - kept in the arena until its exits
/// settle, then freed and reported by the "spatialNodeFreed" event - or
/// was freed on the spot (false), the caller's cue to clean up now.
fn exit_node(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<bool> {
  super::gui(&ctx).alloy.spatial().exit(id).map_err(|e| throw_str(&ctx, &format!("exitNode: {e}")))
}

/// The node's lifecycle state for probing: `{ leaving, shown, motion }`,
/// `motion` one `{ component, to, heldUntil }` per track running or write
/// held on it (`to` the target lanes, `heldUntil` the clock a held write
/// applies at, null for a running track).
fn describe_node<'js>(ctx: Ctx<'js>, id: u64) -> rquickjs::Result<Object<'js>> {
  let st = super::gui(&ctx);
  let spatial = st.alloy.spatial();
  let leaving = spatial.leaving(id).map_err(|e| throw_str(&ctx, &format!("describeNode: {e}")))?;
  let shown = spatial.shown(id).map_err(|e| throw_str(&ctx, &format!("describeNode: {e}")))?;
  let motion = spatial.motion_of(id).map_err(|e| throw_str(&ctx, &format!("describeNode: {e}")))?;
  let obj = Object::new(ctx.clone())?;
  obj.set("leaving", leaving)?;
  obj.set("shown", shown)?;
  let list = rquickjs::Array::new(ctx.clone())?;
  for (i, m) in motion.iter().enumerate() {
    let entry = Object::new(ctx.clone())?;
    entry.set("component", component_name(m.component))?;
    let lanes = if m.component == Component::Rotation { 4 } else { 3 };
    entry.set("to", m.to[..lanes].iter().map(|&x| x as f64).collect::<Vec<f64>>())?;
    match m.held_until_ms {
      Some(at) => entry.set("heldUntil", at)?,
      None => entry.set("heldUntil", rquickjs::Null)?,
    }
    list.set(i, entry)?;
  }
  obj.set("motion", list)?;
  Ok(obj)
}

fn component_name(component: Component) -> &'static str {
  match component {
    Component::Position => "position",
    Component::Rotation => "rotation",
    Component::Scale => "scale",
  }
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
/// speak the element transition vocabulary whole - `from`/`exit` on a
/// component entry are its enter and exit values, the lanes of that
/// component, bare or in the endpoint object form; `stagger` (ms) makes
/// the node a stagger group for descendant enters and exits - or a bare
/// shorthand string as the `all` catch-all.
fn decode_node_transition(value: &PropValue) -> Result<NodeTransitionConfig, String> {
  if value.as_str().is_some() {
    return Ok(NodeTransitionConfig { all: Some(decode_node_motion("transition", value)?), ..Default::default() });
  }
  let entries = value.as_map().ok_or_else(|| {
    "transition must be a shorthand string or an object keyed by component (position, rotation, scale, all)".to_string()
  })?;
  let mut config = NodeTransitionConfig::default();
  for (key, entry) in entries {
    let at = format!("transition.{key}");
    match key.as_str() {
      "position" => config.position = Some(node_entry::<3>(decode_node_entry(&at, entry, Some(3))?)),
      "scale" => config.scale = Some(node_entry::<3>(decode_node_entry(&at, entry, Some(3))?)),
      "rotation" => config.rotation = Some(node_entry::<4>(decode_node_entry(&at, entry, Some(4))?)),
      "all" => config.all = Some(decode_node_motion(&at, entry)?),
      "stagger" => config.stagger_ms = Some(decode_stagger(entry)?),
      other => {
        return Err(format!(
          "transition.{other}: '{other}' is not a transform component (expected position, rotation, scale, all or stagger)"
        ))
      }
    }
  }
  Ok(config)
}

/// A decoded entry in the arena's per-component shape; the decoder checked
/// the lane counts, so the arrays fill exactly.
fn node_entry<const N: usize>(d: NodeEntryDecoded) -> NodeTransitionEntry<N> {
  let endpoint = |(lanes, motion): (Vec<f32>, NodeMotion)| {
    let mut value = [0.0f32; N];
    value.copy_from_slice(&lanes);
    NodeEndpoint { value, motion }
  };
  NodeTransitionEntry { motion: d.motion, from: d.from.map(endpoint), exit: d.exit.map(endpoint) }
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
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_cull(id, enabled, margin as f32)
    .map_err(|e| throw_str(&ctx, &format!("setCull: {e}")))
}

fn set_cull_bounds(ctx: Ctx<'_>, id: u64, bounds: OptArg<TypedArray<'_, f32>>) -> rquickjs::Result<()> {
  let b = box_arg(&ctx, &bounds, "setCullBounds")?;
  super::gui(&ctx).alloy.spatial().set_cull_bounds(id, b).map_err(|e| throw_str(&ctx, &format!("setCullBounds: {e}")))
}

fn set_cull_group(ctx: Ctx<'_>, id: u64, members: Vec<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_cull_group(id, &members)
    .map_err(|e| throw_str(&ctx, &format!("setCullGroup: {e}")))
}

/// Positions (and uvs, when `uv_offset` is not -1) gathered out of an
/// interleaved vertex array: `stride` floats per vertex, xyz at
/// `pos_offset`, uv at `uv_offset`.
fn gather_vertices<'js>(
  ctx: &Ctx<'js>,
  vertices: &TypedArray<'js, f32>,
  stride: u32,
  pos_offset: u32,
  uv_offset: i32,
  api: &str,
) -> rquickjs::Result<(Vec<f32>, Option<Vec<f32>>)> {
  let v = floats(ctx, vertices, api)?;
  let stride = stride as usize;
  if stride < 3 || pos_offset as usize + 3 > stride || (uv_offset >= 0 && uv_offset as usize + 2 > stride) {
    return Err(throw_str(ctx, &format!("{api}: offsets do not fit the stride")));
  }
  let count = v.len() / stride;
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
  Ok((positions, uvs))
}

/// A geometry's positions for the core: read from an interleaved vertex
/// array (`stride` floats per vertex, xyz at `posOffset`, uv at
/// `uvOffset` or -1 for none), with `indices` a Uint16Array or
/// Uint32Array triangle list for the narrowphase, or absent for a shape
/// that only gives its box. Returns the shape id.
fn create_shape<'js>(
  ctx: Ctx<'js>,
  vertices: TypedArray<'js, f32>,
  stride: u32,
  pos_offset: u32,
  uv_offset: i32,
  indices: OptArg<Value<'js>>,
) -> rquickjs::Result<u64> {
  let (positions, uvs) = gather_vertices(&ctx, &vertices, stride, pos_offset, uv_offset, "createShape")?;
  let indices: Vec<u32> = match indices.0 {
    None => Vec::new(),
    Some(indices) => {
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
      }
    }
  };
  super::gui(&ctx)
    .alloy
    .spatial()
    .create_shape(Shape { positions, uvs, indices })
    .map_err(|e| throw_str(&ctx, &format!("createShape: {e}")))
}

/// Rewrite the shape's vertices from `first` on with the ones in
/// `vertices`, laid out as for createShape (uvs exactly when the shape
/// has them).
fn update_shape<'js>(
  ctx: Ctx<'js>,
  id: u64,
  vertices: TypedArray<'js, f32>,
  stride: u32,
  pos_offset: u32,
  uv_offset: i32,
  first: u32,
) -> rquickjs::Result<()> {
  let (positions, uvs) = gather_vertices(&ctx, &vertices, stride, pos_offset, uv_offset, "updateShape")?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .update_shape(id, first as usize, &positions, uvs.as_deref())
    .map_err(|e| throw_str(&ctx, &format!("updateShape: {e}")))
}

fn destroy_shape(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_shape(id).map_err(|e| throw_str(&ctx, &format!("destroyShape: {e}")))
}

fn set_shape(ctx: Ctx<'_>, id: u64, shape: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_shape(id, shape.0).map_err(|e| throw_str(&ctx, &format!("setShape: {e}")))
}

/// The node's layer mask (default 1), what a query's `layers` is tested
/// against.
fn set_layers(ctx: Ctx<'_>, id: u64, layers: u32) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_layers(id, layers).map_err(|e| throw_str(&ctx, &format!("setLayers: {e}")))
}

/// The trailing filter object every query takes, `{ root?, layers?,
/// nodes? }`: the subtree root, the layer mask and the node include-list,
/// each optional.
fn filter_arg(filter: OptArg<Object<'_>>) -> rquickjs::Result<QueryFilter> {
  let Some(f) = filter.0 else {
    return Ok(QueryFilter::default());
  };
  Ok(QueryFilter {
    root: f.get::<_, Option<u64>>("root")?,
    layers: f.get::<_, Option<u32>>("layers")?,
    nodes: f.get::<_, Option<Vec<u64>>>("nodes")?,
  })
}

/// Every shown node with bounds the ray strikes, nearest first, as
/// `{ node, distance, point, normal, face?, uv? }` objects.
fn raycast<'js>(
  ctx: Ctx<'js>,
  origin: TypedArray<'js, f32>,
  direction: TypedArray<'js, f32>,
  filter: OptArg<Object<'js>>,
) -> rquickjs::Result<Array<'js>> {
  let o = floats(&ctx, &origin, "raycast")?;
  let d = floats(&ctx, &direction, "raycast")?;
  if o.len() != 3 || d.len() != 3 {
    return Err(throw_str(&ctx, "raycast: origin and direction must be Float32Arrays of 3"));
  }
  let filter = filter_arg(filter)?;
  let hits = super::gui(&ctx)
    .alloy
    .spatial()
    .raycast([o[0], o[1], o[2]], [d[0], d[1], d[2]], &filter)
    .map_err(|e| throw_str(&ctx, &format!("raycast: {e}")))?;
  let arr = Array::new(ctx.clone())?;
  for (i, h) in hits.iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("node", h.node)?;
    obj.set("distance", h.distance as f64)?;
    obj.set("point", vec3(h.point))?;
    obj.set("normal", vec3(h.normal))?;
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

/// A query volume from its kind and packed floats: "capsule" is a, b,
/// radius (7 floats; a sphere when a == b), "box" is center, half
/// extents, rotation quaternion (10).
fn volume_arg(ctx: &Ctx<'_>, kind: &str, data: &TypedArray<'_, f32>, api: &str) -> rquickjs::Result<Volume> {
  let v = floats(ctx, data, api)?;
  match (kind, v.len()) {
    ("capsule", 7) => Ok(Volume::Capsule { a: [v[0], v[1], v[2]], b: [v[3], v[4], v[5]], radius: v[6] }),
    ("box", 10) => Ok(Volume::Box {
      center: [v[0], v[1], v[2]],
      half: [v[3], v[4], v[5]],
      rotation: [v[6], v[7], v[8], v[9]],
    }),
    _ => Err(throw_str(
      ctx,
      &format!("{api}: volume must be \"capsule\" with 7 floats (a, b, radius) or \"box\" with 10 (center, half extents, rotation)"),
    )),
  }
}

fn vec3(v: [f32; 3]) -> Vec<f64> {
  vec![v[0] as f64, v[1] as f64, v[2] as f64]
}

/// Every shown node with bounds the volume touches, each with its deepest
/// contact, as `{ node, point, normal, depth }` objects (unordered).
fn overlap<'js>(
  ctx: Ctx<'js>,
  kind: String,
  data: TypedArray<'js, f32>,
  filter: OptArg<Object<'js>>,
) -> rquickjs::Result<Array<'js>> {
  let volume = volume_arg(&ctx, &kind, &data, "overlap")?;
  let filter = filter_arg(filter)?;
  let hits = super::gui(&ctx)
    .alloy
    .spatial()
    .overlap(&volume, &filter)
    .map_err(|e| throw_str(&ctx, &format!("overlap: {e}")))?;
  let arr = Array::new(ctx.clone())?;
  for (i, h) in hits.iter().enumerate() {
    let obj = Object::new(ctx.clone())?;
    obj.set("node", h.node)?;
    obj.set("point", vec3(h.point))?;
    obj.set("normal", vec3(h.normal))?;
    obj.set("depth", h.depth as f64)?;
    arr.set(i, obj)?;
  }
  Ok(arr)
}

/// The volume moved by `motion`: every shown node with bounds it touches
/// on the way, at its first touch, earliest first, as `{ node, time,
/// point, normal }` objects.
fn sweep<'js>(
  ctx: Ctx<'js>,
  kind: String,
  data: TypedArray<'js, f32>,
  motion: TypedArray<'js, f32>,
  filter: OptArg<Object<'js>>,
) -> rquickjs::Result<Array<'js>> {
  let volume = volume_arg(&ctx, &kind, &data, "sweep")?;
  let m = floats(&ctx, &motion, "sweep")?;
  if m.len() != 3 {
    return Err(throw_str(&ctx, "sweep: motion must be a Float32Array of 3"));
  }
  let filter = filter_arg(filter)?;
  let hits = super::gui(&ctx)
    .alloy
    .spatial()
    .sweep(&volume, [m[0], m[1], m[2]], &filter)
    .map_err(|e| throw_str(&ctx, &format!("sweep: {e}")))?;
  let arr = Array::new(ctx.clone())?;
  for (i, h) in hits.iter().enumerate() {
    arr.set(i, impact_object(&ctx, h)?)?;
  }
  Ok(arr)
}

/// One sweep/moveAndSlide impact as a `{ node, time, point, normal }` object.
fn impact_object<'js>(ctx: &Ctx<'js>, h: &alloy::spatial::Impact) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("node", h.node)?;
  obj.set("time", h.time as f64)?;
  obj.set("point", vec3(h.point))?;
  obj.set("normal", vec3(h.normal))?;
  Ok(obj)
}

/// The mover's options from a JS object, each field optional over the
/// core's defaults: `up` (an array of 3), `floorMaxAngle`, `maxSlides`,
/// `skin`, `floorSnap`.
fn move_options(ctx: &Ctx<'_>, opts: OptArg<Object<'_>>) -> rquickjs::Result<MoveOptions> {
  let mut out = MoveOptions::default();
  let Some(opts) = opts.0 else {
    return Ok(out);
  };
  if let Some(up) = opts.get::<_, Option<Vec<f32>>>("up")? {
    if up.len() != 3 {
      return Err(throw_str(ctx, "moveAndSlide: up must be an array of 3"));
    }
    out.up = [up[0], up[1], up[2]];
  }
  if let Some(v) = opts.get::<_, Option<f32>>("floorMaxAngle")? {
    out.floor_max_angle = v;
  }
  if let Some(v) = opts.get::<_, Option<u32>>("maxSlides")? {
    out.max_slides = v;
  }
  if let Some(v) = opts.get::<_, Option<f32>>("skin")? {
    out.skin = v;
  }
  if let Some(v) = opts.get::<_, Option<f32>>("floorSnap")? {
    out.floor_snap = v;
  }
  Ok(out)
}

/// The volume moved by `motion` through the admitted nodes, sliding along
/// what it hits (the core's move_and_slide), as `{ motion, floor, wall,
/// ceiling, hits }`: `floor` the floor normal or null, `hits` the
/// impacts met in order.
fn move_and_slide<'js>(
  ctx: Ctx<'js>,
  kind: String,
  data: TypedArray<'js, f32>,
  motion: TypedArray<'js, f32>,
  opts: OptArg<Object<'js>>,
  filter: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let volume = volume_arg(&ctx, &kind, &data, "moveAndSlide")?;
  let m = floats(&ctx, &motion, "moveAndSlide")?;
  if m.len() != 3 {
    return Err(throw_str(&ctx, "moveAndSlide: motion must be a Float32Array of 3"));
  }
  let options = move_options(&ctx, opts)?;
  let filter = filter_arg(filter)?;
  let r = super::gui(&ctx)
    .alloy
    .spatial()
    .move_and_slide(&volume, [m[0], m[1], m[2]], &options, &filter)
    .map_err(|e| throw_str(&ctx, &format!("moveAndSlide: {e}")))?;
  let obj = Object::new(ctx.clone())?;
  obj.set("motion", vec3(r.motion))?;
  match r.floor {
    Some(n) => obj.set("floor", vec3(n))?,
    None => obj.set("floor", Value::new_null(ctx.clone()))?,
  }
  obj.set("wall", r.wall)?;
  obj.set("ceiling", r.ceiling)?;
  let hits = Array::new(ctx.clone())?;
  for (i, h) in r.hits.iter().enumerate() {
    hits.set(i, impact_object(&ctx, h)?)?;
  }
  obj.set("hits", hits)?;
  Ok(obj)
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
      other => return Err(throw_str(&ctx, &format!("createClip: channel {i} interpolation {other} is not 0, 1 or 2"))),
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

/// Bind root motion to a player: `channel` of `clip` (a position channel,
/// normally the authored clip's root track) is sampled per advance and
/// its delta reported as a "spatialRootMotion" event, with the twist of
/// `rotation` (a rotation channel of the same clip) about `up` when
/// given; with an `anchor` the delta also moves and turns that node.
/// `opts`: { up?: [x, y, z] (default +y), vertical?: boolean (default
/// true; false keeps the height out of the delta) }.
fn bind_root_motion<'js>(
  ctx: Ctx<'js>,
  id: u64,
  clip: u64,
  channel: u32,
  rotation: OptArg<u32>,
  anchor: OptArg<u64>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<()> {
  let mut up = [0.0f32, 1.0, 0.0];
  let mut vertical = true;
  if let Some(o) = opts.0 {
    if let Some(v) = o.get::<_, Option<Vec<f64>>>("up")? {
      if v.len() != 3 {
        return Err(throw_str(&ctx, "bindRootMotion: up must be [x, y, z]"));
      }
      up = [v[0] as f32, v[1] as f32, v[2] as f32];
    }
    if let Some(v) = o.get::<_, Option<bool>>("vertical")? {
      vertical = v;
    }
  }
  super::gui(&ctx)
    .alloy
    .spatial()
    .bind_root_motion(id, RootMotion { clip, channel, rotation: rotation.0, anchor: anchor.0, up, vertical })
    .map_err(|e| throw_str(&ctx, &format!("bindRootMotion: {e}")))
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
    .spatial_bind_record(id, Some(sink), None)
    .map_err(|e| throw_str(&ctx, &format!("bindPoseRecord: {e}")))
}

/// Bind the node's instance-record sink with the matrix projection: the
/// flush writes the node's world matrix, relative to `anchor` when given,
/// as 16 floats to record slot `index` of vertex buffer `buffer`.
fn bind_matrix_record(ctx: Ctx<'_>, id: u64, buffer: u64, index: u32, anchor: OptArg<u64>) -> rquickjs::Result<()> {
  let sink = InstanceRecordSink { buffer, index, projection: InstanceProjection::Matrix };
  super::gui(&ctx)
    .alloy
    .spatial_bind_record(id, Some(sink), anchor.0)
    .map_err(|e| throw_str(&ctx, &format!("bindMatrixRecord: {e}")))
}

fn unbind_record(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial_bind_record(id, None, None).map_err(|e| throw_str(&ctx, &format!("unbindRecord: {e}")))
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
/// same frame's turn. Root-motion deltas of bound players follow as one
/// "spatialRootMotion" event each, payload `{ player, x, y, z, yaw }`.
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
  for (player, delta, yaw) in st.alloy.spatial().take_root_motion() {
    let obj = Object::new(ctx.clone()).expect("create spatialRootMotion object");
    obj.set("player", player).expect("set player");
    obj.set("x", delta[0] as f64).expect("set x");
    obj.set("y", delta[1] as f64).expect("set y");
    obj.set("z", delta[2] as f64).expect("set z");
    obj.set("yaw", yaw as f64).expect("set yaw");
    crate::emit_event(ctx, "spatialRootMotion", obj);
  }
  PlayersTick { active: tick.active, wrote: tick.wrote }
}

/// Advance the node transitions to the stamped clock and publish what
/// moved: steps every running track (writing node TRS through the arena's
/// ordinary snap path), flushes the arena when anything was written, and
/// emits one "spatialTransitionEnd" engine event per settled track,
/// payload `{ node, component }`, then one "spatialNodeFreed" per leaving
/// node the advance freed. `frame::draw` calls this beside the render
/// tree's transition advance, before the frame's demand gate.
pub(crate) fn tick(ctx: &Ctx<'_>) -> SpatialTick {
  let Some(st) = super::try_gui(ctx) else {
    return SpatialTick { active: false, wrote: false };
  };
  let active = st.alloy.spatial().advance_transitions();
  let settled = st.alloy.spatial().take_settled_transitions();
  let freed = st.alloy.spatial().take_freed();
  // The flush is unconditional: besides transition writes, the queue may
  // hold clip-player poses (advanced before the frame's JS) and whatever
  // that JS wrote without its own microtask flush landing yet. An empty
  // queue is a cheap no-op.
  let wrote = st.alloy.spatial_flush();
  for (node, component) in settled {
    let obj = Object::new(ctx.clone()).expect("create spatialTransitionEnd object");
    obj.set("node", node).expect("set node");
    obj.set("component", component_name(component)).expect("set component");
    crate::emit_event(ctx, "spatialTransitionEnd", obj);
  }
  // Leaving nodes the advance freed: their slot-zeroing writes landed in
  // the flush above, so a consumer recycling a record slot on this event
  // never races the corpse's last frame. One "spatialNodeFreed" per node,
  // payload `{ node }`.
  for node in freed {
    let obj = Object::new(ctx.clone()).expect("create spatialNodeFreed object");
    obj.set("node", node).expect("set node");
    crate::emit_event(ctx, "spatialNodeFreed", obj);
  }
  SpatialTick { active, wrote }
}
