//! JS bindings for the spatial core (`alloy::spatial`): node create/move/
//! destroy, draw-sink binding and the flush. A thin marshalling layer - the
//! transform is one Float32Array of 10 (position xyz, quaternion xyzw,
//! scale xyz) so a hot-path write is one argument, and node ids are plain
//! numbers (generation-tagged, never reused).

use std::rc::Rc;

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, FromJs, Function, JsLifetime, Object, TypedArray, Value};

use crate::alloy_plugins::properties::transition::{
  decode_node_entry, decode_node_motion, decode_stagger, LaneRule, NodeEntryDecoded,
};
use crate::alloy_plugins::value::PropValue;
use crate::plugins::marshal::{elements_mut_of, elements_of, OptArg};
use alloy::spatial::{
  vertex_normals, write_channel, AdditiveReference, ChainMethod, ChainSpec, ChannelInterpolation, ChannelPath,
  ClipChannel, ClipEvent, ClipMarker, Component, DrawOrder, DrawQueue, DrawSink, FeetSpec, GridSpec, IkLimit, IkPoint,
  IkSolverSpec, IkTarget, IkUp, InstanceProjection, InstanceRecordSink, LayerMode, LayerSpec, LayerUpdate, LegSpec,
  LodLevel, LookAtBone, LookAtLimit, LookAtSpec, MarkerPhase, MoveOptions, NodeEndpoint, NodeMotion,
  NodeTransitionConfig, NodeTransitionEntry, Occluder, PlayerSpec, PlayerUpdate, Projection, QueryFilter, RegionEvent,
  RegionMargins, RegionRef, RegionSpec, RegionState, RestPose, RootMotion, Shape, SharedSlotSink, SpringCollider,
  SpringJoint, SpringJointUpdate, SpringShape, SyncMode, TextureSlotSink, TwoBoneSpec, Volume,
};

/// Floats per joint in createSpring's joint payload (see `create_spring`).
const SPRING_JOINT_FLOATS: usize = 10;
/// Floats per collider in createSpring's collider payload.
const SPRING_COLLIDER_FLOATS: usize = 9;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

// The spatial bindings keep no state of their own: every call forwards to
// the arena in the shared alloy context (`super::gui`).

/// The 10 floats of a transform argument: position, quaternion, scale.
fn transform(ctx: &Ctx<'_>, data: &TypedArray<'_, f32>, api: &str) -> rquickjs::Result<([f32; 3], [f32; 4], [f32; 3])> {
  let f = elements_of(ctx, data, api)?;
  if f.len() != 10 {
    return Err(throw_str(
      ctx,
      &format!("{api}: transform must be a Float32Array of 10 (position, quaternion, scale)"),
    ));
  }
  Ok(([f[0], f[1], f[2]], [f[3], f[4], f[5], f[6]], [f[7], f[8], f[9]]))
}

pub struct SpatialModule;

// The engine's hold on alloy's spatial core, whose scene its app builds:
// nodes, shapes, sinks, clips and players. One engine at a time fills the
// core, so the hold's drop empties it whole (`reset_spatial`) rather than
// retracing ids: the app's GPU targets die with its engine, and a sink or
// looping player left behind would write into nothing every frame.
struct Inner {
  gui: Rc<super::Gui>,
}

impl Drop for Inner {
  fn drop(&mut self) {
    self.gui.alloy.reset_spatial();
  }
}

// Held for its drop alone, so never read.
#[derive(Clone, JsLifetime)]
struct SpatialPluginState(
  #[qjs(skip_trace)]
  #[allow(dead_code)]
  Rc<Inner>,
);

/// Take the engine's hold on the spatial core (see `Inner`). Runs at engine
/// init, before any module import.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(SpatialPluginState(Rc::new(Inner { gui: super::gui(ctx) }))).expect("store spatial state");
}

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
    decl.declare("setTimeScale")?;
    decl.declare("timeRate")?;
    decl.declare("setVisible")?;
    decl.declare("bindDraw")?;
    decl.declare("unbindDraw")?;
    decl.declare("setDrawCount")?;
    decl.declare("setDrawKey")?;
    decl.declare("setDrawSort")?;
    decl.declare("worldMatrix")?;
    decl.declare("shown")?;
    decl.declare("flush")?;
    decl.declare("startTransitions")?;
    decl.declare("setBounds")?;
    decl.declare("setView")?;
    decl.declare("setLodBias")?;
    decl.declare("setLodFade")?;
    decl.declare("lodLevel")?;
    decl.declare("setLodReference")?;
    decl.declare("setCull")?;
    decl.declare("setCullBounds")?;
    decl.declare("setCullGroup")?;
    decl.declare("setOccluder")?;
    decl.declare("setOcclusion")?;
    decl.declare("cullState")?;
    decl.declare("occlusionStats")?;
    decl.declare("allocViewKey")?;
    decl.declare("setRegion")?;
    decl.declare("createGrid")?;
    decl.declare("destroyGrid")?;
    decl.declare("setGridWeight")?;
    decl.declare("setGridLimit")?;
    decl.declare("setCellWeight")?;
    decl.declare("setRegionWeight")?;
    decl.declare("cellState")?;
    decl.declare("regionState")?;
    decl.declare("setResidencyBudget")?;
    decl.declare("setResidencyLimit")?;
    decl.declare("setLod")?;
    decl.declare("createShape")?;
    decl.declare("updateShape")?;
    decl.declare("computeNormals")?;
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
    decl.declare("bindWeightsSlot")?;
    decl.declare("unbindWeightsSlot")?;
    decl.declare("setWeights")?;
    decl.declare("writeWeights")?;
    decl.declare("readWeights")?;
    decl.declare("createClip")?;
    decl.declare("destroyClip")?;
    decl.declare("setClipMarkers")?;
    decl.declare("deriveAdditiveClip")?;
    decl.declare("setRest")?;
    decl.declare("createRig")?;
    decl.declare("destroyRig")?;
    decl.declare("createLayer")?;
    decl.declare("setLayer")?;
    decl.declare("destroyLayer")?;
    decl.declare("createMask")?;
    decl.declare("setMask")?;
    decl.declare("destroyMask")?;
    decl.declare("createPlayer")?;
    decl.declare("setPlayer")?;
    decl.declare("readPlayer")?;
    decl.declare("markerInside")?;
    decl.declare("bindRootMotion")?;
    decl.declare("destroyPlayer")?;
    decl.declare("createGroup")?;
    decl.declare("setGroup")?;
    decl.declare("destroyGroup")?;
    decl.declare("inertialize")?;
    decl.declare("readCurve")?;
    decl.declare("bindCurveParam")?;
    decl.declare("unbindCurveParam")?;
    decl.declare("readTransform")?;
    decl.declare("createSpring")?;
    decl.declare("destroySpring")?;
    decl.declare("resetSpring")?;
    decl.declare("setSpringJoint")?;
    decl.declare("setSpringForce")?;
    decl.declare("createIk")?;
    decl.declare("destroyIk")?;
    decl.declare("setIkSolver")?;
    decl.declare("setIkTarget")?;
    decl.declare("setIkWeight")?;
    decl.declare("bindIkWeightCurve")?;
    decl.declare("readIkReach")?;
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
    exports.export("setTimeScale", Function::new(ctx.clone(), set_time_scale)?)?;
    exports.export("timeRate", Function::new(ctx.clone(), time_rate)?)?;
    exports.export("setVisible", Function::new(ctx.clone(), set_visible)?)?;
    exports.export("bindDraw", Function::new(ctx.clone(), bind_draw)?)?;
    exports.export("unbindDraw", Function::new(ctx.clone(), unbind_draw)?)?;
    exports.export("setDrawCount", Function::new(ctx.clone(), set_draw_count)?)?;
    exports.export("setDrawKey", Function::new(ctx.clone(), set_draw_key)?)?;
    exports.export("setDrawSort", Function::new(ctx.clone(), set_draw_sort)?)?;
    exports.export("worldMatrix", Function::new(ctx.clone(), world_matrix)?)?;
    exports.export("shown", Function::new(ctx.clone(), shown)?)?;
    exports.export("flush", Function::new(ctx.clone(), flush)?)?;
    exports.export("startTransitions", Function::new(ctx.clone(), start_transitions)?)?;
    exports.export("setBounds", Function::new(ctx.clone(), set_bounds)?)?;
    exports.export("setView", Function::new(ctx.clone(), set_view)?)?;
    exports.export("setLodBias", Function::new(ctx.clone(), set_lod_bias)?)?;
    exports.export("setLodFade", Function::new(ctx.clone(), set_lod_fade)?)?;
    exports.export("lodLevel", Function::new(ctx.clone(), lod_level)?)?;
    exports.export("setLodReference", Function::new(ctx.clone(), set_lod_reference)?)?;
    exports.export("setCull", Function::new(ctx.clone(), set_cull)?)?;
    exports.export("setCullBounds", Function::new(ctx.clone(), set_cull_bounds)?)?;
    exports.export("setCullGroup", Function::new(ctx.clone(), set_cull_group)?)?;
    exports.export("setOccluder", Function::new(ctx.clone(), set_occluder)?)?;
    exports.export("setOcclusion", Function::new(ctx.clone(), set_occlusion)?)?;
    exports.export("cullState", Function::new(ctx.clone(), cull_state)?)?;
    exports.export("occlusionStats", Function::new(ctx.clone(), occlusion_stats)?)?;
    exports.export("allocViewKey", Function::new(ctx.clone(), alloc_view_key)?)?;
    exports.export("setRegion", Function::new(ctx.clone(), set_region)?)?;
    exports.export("createGrid", Function::new(ctx.clone(), create_grid)?)?;
    exports.export("destroyGrid", Function::new(ctx.clone(), destroy_grid)?)?;
    exports.export("setGridWeight", Function::new(ctx.clone(), set_grid_weight)?)?;
    exports.export("setGridLimit", Function::new(ctx.clone(), set_grid_limit)?)?;
    exports.export("setCellWeight", Function::new(ctx.clone(), set_cell_weight)?)?;
    exports.export("setRegionWeight", Function::new(ctx.clone(), set_region_weight)?)?;
    exports.export("cellState", Function::new(ctx.clone(), cell_state)?)?;
    exports.export("regionState", Function::new(ctx.clone(), region_state)?)?;
    exports.export("setResidencyBudget", Function::new(ctx.clone(), set_residency_budget)?)?;
    exports.export("setResidencyLimit", Function::new(ctx.clone(), set_residency_limit)?)?;
    exports.export("setLod", Function::new(ctx.clone(), set_lod)?)?;
    exports.export("createShape", Function::new(ctx.clone(), create_shape)?)?;
    exports.export("updateShape", Function::new(ctx.clone(), update_shape)?)?;
    exports.export("computeNormals", Function::new(ctx.clone(), compute_normals)?)?;
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
    exports.export("bindWeightsSlot", Function::new(ctx.clone(), bind_weights_slot)?)?;
    exports.export("unbindWeightsSlot", Function::new(ctx.clone(), unbind_weights_slot)?)?;
    exports.export("setWeights", Function::new(ctx.clone(), set_weights)?)?;
    exports.export("writeWeights", Function::new(ctx.clone(), write_weights)?)?;
    exports.export("readWeights", Function::new(ctx.clone(), read_weights)?)?;
    exports.export("createClip", Function::new(ctx.clone(), create_clip)?)?;
    exports.export("destroyClip", Function::new(ctx.clone(), destroy_clip)?)?;
    exports.export("setClipMarkers", Function::new(ctx.clone(), set_clip_markers)?)?;
    exports.export("deriveAdditiveClip", Function::new(ctx.clone(), derive_additive_clip)?)?;
    exports.export("setRest", Function::new(ctx.clone(), set_rest)?)?;
    exports.export("createRig", Function::new(ctx.clone(), create_rig)?)?;
    exports.export("destroyRig", Function::new(ctx.clone(), destroy_rig)?)?;
    exports.export("createLayer", Function::new(ctx.clone(), create_layer)?)?;
    exports.export("setLayer", Function::new(ctx.clone(), set_layer)?)?;
    exports.export("destroyLayer", Function::new(ctx.clone(), destroy_layer)?)?;
    exports.export("createMask", Function::new(ctx.clone(), create_mask)?)?;
    exports.export("setMask", Function::new(ctx.clone(), set_mask)?)?;
    exports.export("destroyMask", Function::new(ctx.clone(), destroy_mask)?)?;
    exports.export("createPlayer", Function::new(ctx.clone(), create_player)?)?;
    exports.export("setPlayer", Function::new(ctx.clone(), set_player)?)?;
    exports.export("readPlayer", Function::new(ctx.clone(), read_player)?)?;
    exports.export("markerInside", Function::new(ctx.clone(), marker_inside)?)?;
    exports.export("bindRootMotion", Function::new(ctx.clone(), bind_root_motion)?)?;
    exports.export("destroyPlayer", Function::new(ctx.clone(), destroy_player)?)?;
    exports.export("createGroup", Function::new(ctx.clone(), create_group)?)?;
    exports.export("setGroup", Function::new(ctx.clone(), set_group)?)?;
    exports.export("destroyGroup", Function::new(ctx.clone(), destroy_group)?)?;
    exports.export("inertialize", Function::new(ctx.clone(), inertialize)?)?;
    exports.export("readCurve", Function::new(ctx.clone(), read_curve)?)?;
    exports.export("bindCurveParam", Function::new(ctx.clone(), bind_curve_param)?)?;
    exports.export("unbindCurveParam", Function::new(ctx.clone(), unbind_curve_param)?)?;
    exports.export("readTransform", Function::new(ctx.clone(), read_transform)?)?;
    exports.export("createSpring", Function::new(ctx.clone(), create_spring)?)?;
    exports.export("destroySpring", Function::new(ctx.clone(), destroy_spring)?)?;
    exports.export("resetSpring", Function::new(ctx.clone(), reset_spring)?)?;
    exports.export("setSpringJoint", Function::new(ctx.clone(), set_spring_joint)?)?;
    exports.export("setSpringForce", Function::new(ctx.clone(), set_spring_force)?)?;
    exports.export("createIk", Function::new(ctx.clone(), create_ik)?)?;
    exports.export("destroyIk", Function::new(ctx.clone(), destroy_ik)?)?;
    exports.export("setIkSolver", Function::new(ctx.clone(), set_ik_solver)?)?;
    exports.export("setIkTarget", Function::new(ctx.clone(), set_ik_target)?)?;
    exports.export("setIkWeight", Function::new(ctx.clone(), set_ik_weight)?)?;
    exports.export("bindIkWeightCurve", Function::new(ctx.clone(), bind_ik_weight_curve)?)?;
    exports.export("readIkReach", Function::new(ctx.clone(), read_ik_reach)?)?;
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
    entry.set("to", m.to.iter().map(|&x| x as f64).collect::<Vec<f64>>())?;
    match m.held_for_ms {
      Some(left) => entry.set("heldFor", left)?,
      None => entry.set("heldFor", rquickjs::Null)?,
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
    Component::Weights => "weights",
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

/// The node transition declaration: an object keyed by component
/// (position, rotation, scale, weights, plus `all` as a catch-all) whose
/// values speak the element transition vocabulary whole - `from`/`exit`
/// on a component entry are its enter and exit values, the lanes of that
/// component (weights: one per morph target), bare or in the endpoint
/// object form; `stagger` (ms) makes the node a stagger group for
/// descendant enters and exits - or a bare shorthand string as the `all`
/// catch-all.
fn decode_node_transition(value: &PropValue) -> Result<NodeTransitionConfig, String> {
  if value.as_str().is_some() {
    return Ok(NodeTransitionConfig { all: Some(decode_node_motion("transition", value)?), ..Default::default() });
  }
  let entries = value.as_map().ok_or_else(|| {
    "transition must be a shorthand string or an object keyed by component (position, rotation, scale, weights, all)"
      .to_string()
  })?;
  let mut config = NodeTransitionConfig::default();
  for (key, entry) in entries {
    let at = format!("transition.{key}");
    match key.as_str() {
      "position" => config.position = Some(node_entry(decode_node_entry(&at, entry, LaneRule::Exactly(3))?, fixed::<3>)),
      "scale" => config.scale = Some(node_entry(decode_node_entry(&at, entry, LaneRule::Exactly(3))?, fixed::<3>)),
      "rotation" => config.rotation = Some(node_entry(decode_node_entry(&at, entry, LaneRule::Exactly(4))?, fixed::<4>)),
      "weights" => config.weights = Some(node_entry(decode_node_entry(&at, entry, LaneRule::Any)?, |lanes| lanes)),
      "all" => config.all = Some(decode_node_motion(&at, entry)?),
      "stagger" => config.stagger_ms = Some(decode_stagger(entry)?),
      other => {
        return Err(format!(
          "transition.{other}: '{other}' is not a node component (expected position, rotation, scale, weights, all or stagger)"
        ))
      }
    }
  }
  Ok(config)
}

/// A decoded entry in the arena's per-component shape, `carrier` turning
/// each endpoint's lanes into the component's value type.
fn node_entry<V>(d: NodeEntryDecoded, carrier: impl Fn(Vec<f32>) -> V) -> NodeTransitionEntry<V> {
  let endpoint = |(lanes, motion): (Vec<f32>, NodeMotion)| NodeEndpoint { value: carrier(lanes), motion };
  NodeTransitionEntry { motion: d.motion, from: d.from.map(endpoint), exit: d.exit.map(endpoint) }
}

/// A fixed-lane component's value; the decoder checked the count, so the
/// array fills exactly.
fn fixed<const N: usize>(lanes: Vec<f32>) -> [f32; N] {
  let mut value = [0.0f32; N];
  value.copy_from_slice(&lanes);
  value
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

/// Declare (null clears) the rate the native motion in the node's subtree
/// runs at. A changed rate requests a frame: a frozen subtree is not
/// demand, so the write that resumes it is what brings the next frame.
fn set_time_scale<'js>(ctx: Ctx<'js>, id: u64, scale: Value<'js>) -> rquickjs::Result<()> {
  let scale = if scale.is_null() || scale.is_undefined() {
    None
  } else {
    match scale.as_number() {
      Some(s) => Some(s as f32),
      None => return Err(throw_str(&ctx, "setTimeScale: scale must be a number or null")),
    }
  };
  let st = super::gui(&ctx);
  let changed =
    st.alloy.spatial().set_time_scale(id, scale).map_err(|e| throw_str(&ctx, &format!("setTimeScale: {e}")))?;
  if changed {
    st.platform.request_frame();
  }
  Ok(())
}

/// The rate the node's motion runs at: its own time scale, else the
/// nearest declaring ancestor's, else 1.
fn time_rate(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<f64> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .time_rate(id)
    .map(|r| r as f64)
    .map_err(|e| throw_str(&ctx, &format!("timeRate: {e}")))
}

fn set_visible(ctx: Ctx<'_>, id: u64, visible: bool) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_visible(id, visible).map_err(|e| throw_str(&ctx, &format!("setVisible: {e}")))
}

/// Route the node's world matrix to one draw entry; the options object
/// carries the entry's uniforms (normal, fade, params), its shown count
/// and its sort key (queue, renderOrder).
fn bind_draw(ctx: Ctx<'_>, id: u64, target: u64, draw: u64, opts: OptArg<Object<'_>>) -> rquickjs::Result<()> {
  let opts = opts.0;
  let field = |name: &str| -> rquickjs::Result<Option<Value<'_>>> {
    match &opts {
      Some(o) => o.get::<_, Option<Value<'_>>>(name),
      None => Ok(None),
    }
  };
  let flag = |name: &str, default: bool| -> rquickjs::Result<bool> {
    match field(name)? {
      Some(v) if !v.is_undefined() => {
        Ok(v.as_bool().ok_or_else(|| throw_str(&ctx, &format!("bindDraw: {name} must be a boolean")))?)
      }
      _ => Ok(default),
    }
  };
  let number = |name: &str, default: f64| -> rquickjs::Result<f64> {
    match field(name)? {
      Some(v) if !v.is_undefined() => {
        Ok(v.as_number().ok_or_else(|| throw_str(&ctx, &format!("bindDraw: {name} must be a number")))?)
      }
      _ => Ok(default),
    }
  };
  let normal = flag("normal", false)?;
  let fade = flag("fade", false)?;
  let params = flag("params", true)?;
  let order_feed = flag("orderFeed", false)?;
  let count = number("count", 1.0)? as u32;
  let queue = draw_queue(&ctx, number("queue", QUEUE_OPAQUE as f64)? as u32, "bindDraw")?;
  let render_order = number("renderOrder", 0.0)? as i32;
  super::gui(&ctx)
    .alloy
    .spatial_bind(
      id,
      DrawSink { target, draw, normal, count, fade, params, order: DrawOrder { queue, render_order }, order_feed },
    )
    .map_err(|e| throw_str(&ctx, &format!("bindDraw: {e}")))
}

/// The JS queue codes of `DrawQueue`, in draw order.
const QUEUE_BACKGROUND: u32 = 0;
const QUEUE_OPAQUE: u32 = 1;
const QUEUE_CUTOUT: u32 = 2;
const QUEUE_TRANSPARENT: u32 = 3;

fn draw_queue(ctx: &Ctx<'_>, code: u32, api: &str) -> rquickjs::Result<DrawQueue> {
  match code {
    QUEUE_BACKGROUND => Ok(DrawQueue::Background),
    QUEUE_OPAQUE => Ok(DrawQueue::Opaque),
    QUEUE_CUTOUT => Ok(DrawQueue::Cutout),
    QUEUE_TRANSPARENT => Ok(DrawQueue::Transparent),
    _ => Err(throw_str(ctx, &format!("{api}: unknown queue {code}"))),
  }
}

/// Re-key every draw sink of the node for the draw sort.
fn set_draw_key(ctx: Ctx<'_>, id: u64, queue: u32, render_order: i32) -> rquickjs::Result<()> {
  let queue = draw_queue(&ctx, queue, "setDrawKey")?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_sink_order(id, DrawOrder { queue, render_order })
    .map_err(|e| throw_str(&ctx, &format!("setDrawKey: {e}")))
}

/// Whether the core orders the target's bound draw entries.
fn set_draw_sort(ctx: Ctx<'_>, target: u64, enabled: bool) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_draw_sort(target, enabled);
  Ok(())
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
  let dst = elements_mut_of(&ctx, &out, "worldMatrix")?;
  if dst.len() != 16 {
    return Err(throw_str(&ctx, "worldMatrix: out must be a Float32Array of 16"));
  }
  dst.copy_from_slice(&world);
  Ok(())
}

fn shown(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<bool> {
  super::gui(&ctx).alloy.spatial().shown(id).map_err(|e| throw_str(&ctx, &format!("shown: {e}")))
}

fn flush(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  // A residency change the flush produced waits for the frame's delivery.
  if st.alloy.spatial_flush() || st.alloy.spatial().residency_demand() {
    st.platform.request_frame();
  }
  Ok(())
}

/// Start the lifecycle motion the frame's callbacks declared (enters of
/// nodes created in the frame, exits of nodes let go of), the frame's
/// start pass: core calls it after the frame callbacks and their reactive
/// flush, before the late pass, so a late-pass read sees the pose the
/// frame draws. Motion it started is demand: the frame is requested.
fn start_transitions(ctx: Ctx<'_>) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  if st.alloy.spatial().start_transitions() {
    st.platform.request_frame();
  }
  Ok(())
}

/// A triangle list argument, a Uint16Array or Uint32Array, widened to u32.
fn index_list<'js>(ctx: &Ctx<'js>, indices: &Value<'js>, api: &str) -> rquickjs::Result<Vec<u32>> {
  if let Some(u16s) = indices.as_object().and_then(|o| TypedArray::<u16>::from_object(o.clone()).ok()) {
    Ok(elements_of(ctx, &u16s, api)?.iter().map(|&i| i as u32).collect())
  } else if let Some(u32s) = indices.as_object().and_then(|o| TypedArray::<u32>::from_object(o.clone()).ok()) {
    Ok(elements_of(ctx, &u32s, api)?.to_vec())
  } else {
    Err(throw_str(ctx, &format!("{api}: indices must be a Uint16Array or Uint32Array")))
  }
}

/// Set (null clears) the node's local tight box [minX, minY, minZ, maxX,
/// maxY, maxZ]; with one the node is in the picking index.
fn set_bounds(ctx: Ctx<'_>, id: u64, bounds: OptArg<TypedArray<'_, f32>>) -> rquickjs::Result<()> {
  let b = match &bounds.0 {
    Some(data) => {
      let f = elements_of(&ctx, data, "setBounds")?;
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
      let f = elements_of(ctx, data, api)?;
      if f.len() != 6 {
        return Err(throw_str(ctx, &format!("{api}: bounds must be a Float32Array of 6 (min xyz, max xyz)")));
      }
      Ok(Some([f[0], f[1], f[2], f[3], f[4], f[5]]))
    }
    None => Ok(None),
  }
}

/// What a target sees: its view and projection as two Float32Arrays of
/// 16 (column-major), or null to forget the target.
fn set_view(
  ctx: Ctx<'_>,
  target: u64,
  view: OptArg<TypedArray<'_, f32>>,
  proj: OptArg<TypedArray<'_, f32>>,
) -> rquickjs::Result<()> {
  let matrices = match (&view.0, &proj.0) {
    (Some(v), Some(p)) => {
      let v = elements_of(&ctx, v, "setView")?;
      let p = elements_of(&ctx, p, "setView")?;
      if v.len() != 16 || p.len() != 16 {
        return Err(throw_str(&ctx, "setView: view and proj must be Float32Arrays of 16"));
      }
      let mut vm = [0.0f32; 16];
      let mut pm = [0.0f32; 16];
      vm.copy_from_slice(v);
      pm.copy_from_slice(p);
      Some((vm, pm))
    }
    (None, None) => None,
    _ => return Err(throw_str(&ctx, "setView: view and proj come together, or null to forget the target")),
  };
  super::gui(&ctx).alloy.spatial().set_view(target, matrices);
  Ok(())
}

/// The level a LOD group draws on a target, or null before it measured.
fn lod_level(ctx: Ctx<'_>, id: u64, target: u64) -> rquickjs::Result<Option<u32>> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .lod_level(id, target)
    .map(|l| l.map(|l| l as u32))
    .map_err(|e| throw_str(&ctx, &format!("lodLevel: {e}")))
}

/// The bias every projected size measured on a target is multiplied by.
fn set_lod_bias(ctx: Ctx<'_>, target: u64, bias: f64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_lod_bias(target, bias as f32);
  Ok(())
}

/// Whether a target cross-fades LOD levels (default true); false switches
/// hard at every threshold there.
fn set_lod_fade(ctx: Ctx<'_>, target: u64, enabled: bool) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_lod_fade(target, enabled);
  Ok(())
}

/// The target whose view a target measures projected size by, or null
/// for its own.
fn set_lod_reference(ctx: Ctx<'_>, target: u64, source: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_lod_reference(target, source.0);
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

/// The node's occluder (null clears it): `{ box }` a Float32Array of 6,
/// `{ quad: [width, height] }`, `{ sphere: { center, radius } }` or
/// `{ mesh: { positions, indices? } }`, in node space.
fn set_occluder<'js>(ctx: Ctx<'js>, id: u64, shape: OptArg<Object<'js>>) -> rquickjs::Result<()> {
  let occluder = match &shape.0 {
    Some(o) => Some(occluder_arg(&ctx, o, "setOccluder")?),
    None => None,
  };
  super::gui(&ctx).alloy.spatial().set_occluder(id, occluder).map_err(|e| throw_str(&ctx, &format!("setOccluder: {e}")))
}

fn occluder_arg<'js>(ctx: &Ctx<'js>, o: &Object<'js>, api: &str) -> rquickjs::Result<Occluder> {
  if let Some(b) = o.get::<_, Option<TypedArray<'js, f32>>>("box")? {
    let f = elements_of(ctx, &b, api)?;
    if f.len() != 6 {
      return Err(throw_str(ctx, &format!("{api}: box must be a Float32Array of 6 (min xyz, max xyz)")));
    }
    return Ok(Occluder::box3([f[0], f[1], f[2], f[3], f[4], f[5]]));
  }
  if let Some(q) = o.get::<_, Option<Vec<f64>>>("quad")? {
    if q.len() != 2 || !(q[0] > 0.0 && q[1] > 0.0) {
      return Err(throw_str(ctx, &format!("{api}: quad must be [width, height], both > 0")));
    }
    return Ok(Occluder::quad(q[0] as f32, q[1] as f32));
  }
  if let Some(s) = o.get::<_, Option<Object<'js>>>("sphere")? {
    let center: Vec<f64> = s.get("center")?;
    let radius: f64 = s.get("radius")?;
    if center.len() != 3 || !(radius > 0.0) {
      return Err(throw_str(ctx, &format!("{api}: sphere needs center [x, y, z] and radius > 0")));
    }
    return Ok(Occluder::sphere([center[0] as f32, center[1] as f32, center[2] as f32], radius as f32));
  }
  if let Some(m) = o.get::<_, Option<Object<'js>>>("mesh")? {
    let positions: TypedArray<'js, f32> = m.get("positions")?;
    let p = elements_of(ctx, &positions, api)?;
    if p.len() % 3 != 0 || p.is_empty() {
      return Err(throw_str(ctx, &format!("{api}: mesh positions must be a non-empty Float32Array of xyz triples")));
    }
    let positions: Vec<[f32; 3]> = p.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let indices: Value<'js> = m.get("indices")?;
    let flat: Vec<u32> = if indices.is_undefined() || indices.is_null() {
      (0..positions.len() as u32).collect()
    } else {
      index_list(ctx, &indices, api)?
    };
    if flat.len() % 3 != 0 {
      return Err(throw_str(ctx, &format!("{api}: mesh indices must come in triples")));
    }
    let indices: Vec<[u32; 3]> = flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    return Occluder::mesh(positions, indices).map_err(|e| throw_str(ctx, &format!("{api}: {e}")));
  }
  Err(throw_str(ctx, &format!("{api}: a shape is {{ box }}, {{ quad }}, {{ sphere }} or {{ mesh }}")))
}

/// Whether a target tests its draw sinks against its occluders' depth
/// buffer (default off).
fn set_occlusion(ctx: Ctx<'_>, target: u64, enabled: bool) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_occlusion(target, enabled);
  Ok(())
}

/// Why the node's draw sink on a target is off, or "on"; null without a
/// sink there.
fn cull_state(ctx: Ctx<'_>, id: u64, target: u64) -> rquickjs::Result<Option<String>> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .cull_state(id, target)
    .map(|s| s.map(|s| s.name().to_string()))
    .map_err(|e| throw_str(&ctx, &format!("cullState: {e}")))
}

/// Occlusion culling's figures over every target.
fn occlusion_stats(ctx: Ctx<'_>) -> rquickjs::Result<Object<'_>> {
  let s = super::gui(&ctx).alloy.spatial().occlusion_stats();
  let obj = Object::new(ctx.clone())?;
  obj.set("targets", s.targets as u32)?;
  obj.set("occluders", s.occluders as u32)?;
  obj.set("triangles", s.triangles as u32)?;
  obj.set("drawn", s.drawn as u32)?;
  obj.set("skipped", s.skipped as u32)?;
  obj.set("builds", s.builds as f64)?;
  obj.set("buildMs", s.build_ms as f64)?;
  obj.set("buildMaxMs", s.build_max_ms as f64)?;
  obj.set("occluded", s.occluded as u32)?;
  Ok(obj)
}

/// Make the node a LOD group: `levels` is an array of `{ node?, size }`
/// objects, nearest first (see `Spatial::set_lod`); an empty array
/// clears. `fade` is the cross-fade band fraction, `reference` the target
/// population levels measure by.
/// A viewer key for a view without a target of its own (a 2d camera).
fn alloc_view_key(ctx: Ctx<'_>) -> u64 {
  super::gui(&ctx).alloy.spatial().alloc_view_key()
}

/// The `{ enter, leave }` margins of a region, from a JS object.
fn margins_arg(ctx: &Ctx<'_>, obj: &Object<'_>, api: &str) -> rquickjs::Result<RegionMargins> {
  let enter: Option<f64> = obj.get("enter")?;
  let leave: Option<f64> = obj.get("leave")?;
  match (enter, leave) {
    (Some(enter), Some(leave)) => Ok(RegionMargins { enter: enter as f32, leave: leave as f32 }),
    _ => Err(throw_str(ctx, &format!("{api}: enter and leave are required numbers"))),
  }
}

/// The `priority` of a region or grid spec: an integer, default 0.
fn priority_arg(ctx: &Ctx<'_>, obj: &Object<'_>, api: &str) -> rquickjs::Result<i32> {
  let priority: Option<f64> = obj.get("priority")?;
  match priority {
    None => Ok(0),
    Some(p) if p.is_finite() && p.fract() == 0.0 && p.abs() <= i32::MAX as f64 => Ok(p as i32),
    Some(_) => Err(throw_str(ctx, &format!("{api}: priority must be an integer"))),
  }
}

/// Make the node a residency region (`{ enter, leave, weight?, priority? }`),
/// or with null end it.
fn set_region<'js>(ctx: Ctx<'js>, id: u64, spec: Value<'js>) -> rquickjs::Result<()> {
  let m = if spec.is_null() || spec.is_undefined() {
    None
  } else {
    let obj = spec
      .as_object()
      .ok_or_else(|| throw_str(&ctx, "setRegion: spec must be { enter, leave, weight?, priority? } or null"))?;
    let margins = margins_arg(&ctx, obj, "setRegion")?;
    let weight: Option<f64> = obj.get("weight")?;
    let priority = priority_arg(&ctx, obj, "setRegion")?;
    Some(RegionSpec { margins, weight: weight.unwrap_or(0.0), priority })
  };
  super::gui(&ctx).alloy.spatial().set_region(id, m).map_err(|e| throw_str(&ctx, &format!("setRegion: {e}")))
}

/// The weight a handler measured for an explicit region once built.
fn set_region_weight(ctx: Ctx<'_>, id: u64, weight: f64) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_region_weight(id, weight)
    .map_err(|e| throw_str(&ctx, &format!("setRegionWeight: {e}")))
}

/// A grid's own cap on resident weight (null = none).
fn set_grid_limit(ctx: Ctx<'_>, grid: u64, limit: Option<f64>) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_grid_limit(grid, limit)
    .map_err(|e| throw_str(&ctx, &format!("setGridLimit: {e}")))
}

/// A cell index `[i, j, k]` argument.
fn cell_arg(ctx: &Ctx<'_>, cell: Vec<i32>, api: &str) -> rquickjs::Result<[i32; 3]> {
  if cell.len() != 3 {
    return Err(throw_str(ctx, &format!("{api}: cell must be [i, j, k]")));
  }
  Ok([cell[0], cell[1], cell[2]])
}

/// The weight a handler measured for a wanted grid cell once built.
fn set_cell_weight(ctx: Ctx<'_>, grid: u64, cell: Vec<i32>, weight: f64) -> rquickjs::Result<()> {
  let cell = cell_arg(&ctx, cell, "setCellWeight")?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_cell_weight(grid, cell, weight)
    .map_err(|e| throw_str(&ctx, &format!("setCellWeight: {e}")))
}

fn state_name(state: Option<RegionState>) -> Option<&'static str> {
  state.map(|s| match s {
    RegionState::Pending => "pending",
    RegionState::Stalled => "stalled",
    RegionState::Resident => "resident",
  })
}

/// Where a grid cell stands: "pending", "stalled", "resident", or null.
fn cell_state(ctx: Ctx<'_>, grid: u64, cell: Vec<i32>) -> rquickjs::Result<Option<&'static str>> {
  let cell = cell_arg(&ctx, cell, "cellState")?;
  let state =
    super::gui(&ctx).alloy.spatial().cell_state(grid, cell).map_err(|e| throw_str(&ctx, &format!("cellState: {e}")))?;
  Ok(state_name(state))
}

/// Where an explicit region stands (see `cell_state`).
fn region_state(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<Option<&'static str>> {
  let state =
    super::gui(&ctx).alloy.spatial().region_state(id).map_err(|e| throw_str(&ctx, &format!("regionState: {e}")))?;
  Ok(state_name(state))
}

/// A fixed-length numeric array field of a JS object, `None` when absent.
fn array_field<T: for<'a> FromJs<'a> + Copy + Default, const N: usize>(
  ctx: &Ctx<'_>,
  obj: &Object<'_>,
  key: &str,
  api: &str,
) -> rquickjs::Result<Option<[T; N]>> {
  let Some(values) = obj.get::<_, Option<Vec<T>>>(key)? else {
    return Ok(None);
  };
  if values.len() != N {
    return Err(throw_str(ctx, &format!("{api}: {key} must be an array of {N}")));
  }
  let mut out = [T::default(); N];
  out.copy_from_slice(&values);
  Ok(Some(out))
}

/// A grid of regions from `{ origin, cell, enter, leave, bounds?, wrap?,
/// weight?, limit?, priority?, label? }`; returns its id.
fn create_grid<'js>(ctx: Ctx<'js>, spec: Object<'js>) -> rquickjs::Result<u64> {
  let origin = array_field::<f32, 3>(&ctx, &spec, "origin", "createGrid")?
    .ok_or_else(|| throw_str(&ctx, "createGrid: origin is required"))?;
  let cell = array_field::<f32, 3>(&ctx, &spec, "cell", "createGrid")?
    .ok_or_else(|| throw_str(&ctx, "createGrid: cell is required"))?;
  let margins = margins_arg(&ctx, &spec, "createGrid")?;
  let bounds = array_field::<i32, 6>(&ctx, &spec, "bounds", "createGrid")?;
  let wrap = array_field::<i32, 3>(&ctx, &spec, "wrap", "createGrid")?;
  let weight: Option<f64> = spec.get("weight")?;
  let limit: Option<f64> = spec.get("limit")?;
  let priority = priority_arg(&ctx, &spec, "createGrid")?;
  let label: Option<String> = spec.get("label")?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .create_grid(GridSpec {
      origin,
      cell,
      margins,
      bounds,
      wrap,
      weight: weight.unwrap_or(1.0),
      limit,
      priority,
      label,
    })
    .map_err(|e| throw_str(&ctx, &format!("createGrid: {e}")))
}

fn destroy_grid(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_grid(id).map_err(|e| throw_str(&ctx, &format!("destroyGrid: {e}")))
}

/// A grid's weight per resident cell.
fn set_grid_weight(ctx: Ctx<'_>, id: u64, weight: f64) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_grid_weight(id, weight)
    .map_err(|e| throw_str(&ctx, &format!("setGridWeight: {e}")))
}

/// The region enters delivered per frame scene-wide.
fn set_residency_budget(ctx: Ctx<'_>, budget: u32) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().set_residency_budget(budget);
  Ok(())
}

/// The scene-wide cap on resident grid weight (null = none).
fn set_residency_limit(ctx: Ctx<'_>, limit: Option<f64>) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_residency_limit(limit)
    .map_err(|e| throw_str(&ctx, &format!("setResidencyLimit: {e}")))
}

fn set_lod<'js>(ctx: Ctx<'js>, id: u64, levels: Array<'js>, fade: f64, reference: OptArg<u64>) -> rquickjs::Result<()> {
  let mut out = Vec::with_capacity(levels.len());
  for entry in levels.iter::<Object<'js>>() {
    let entry = entry?;
    let node = entry.get::<_, Option<u64>>("node")?;
    let size = entry.get::<_, f64>("size")?;
    out.push(LodLevel { node, size: size as f32 });
  }
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_lod(id, &out, fade as f32, reference.0)
    .map_err(|e| throw_str(&ctx, &format!("setLod: {e}")))
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
  let v = elements_of(ctx, vertices, api)?;
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
    Some(indices) => index_list(&ctx, &indices, "createShape")?,
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

/// One channel of an interleaved float vertex array as JS describes it:
/// `{ data: Float32Array, stride, offset }`, stride and offset in floats.
struct Channel<'js> {
  data: TypedArray<'js, f32>,
  stride: u32,
  offset: u32,
}

impl<'js> FromJs<'js> for Channel<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
    let obj = Object::from_js(ctx, value)?;
    Ok(Channel { data: obj.get("data")?, stride: obj.get("stride")?, offset: obj.get("offset")? })
  }
}

/// Recompute vertex normals in place from a triangle list: positions read
/// from the `positions` channel, unit normals written into the `normals`
/// channel (its data may be the same array when the layout interleaves
/// them), the math in `alloy::spatial::vertex_normals`.
fn compute_normals<'js>(
  ctx: Ctx<'js>,
  positions: Channel<'js>,
  normals: Channel<'js>,
  indices: Value<'js>,
) -> rquickjs::Result<()> {
  let indices = index_list(&ctx, &indices, "computeNormals")?;
  let computed = {
    let v = elements_of(&ctx, &positions.data, "computeNormals")?;
    vertex_normals(v, positions.stride as usize, positions.offset as usize, &indices)
      .map_err(|e| throw_str(&ctx, &format!("computeNormals: {e}")))?
  };
  let target = elements_mut_of(&ctx, &normals.data, "computeNormals")?;
  write_channel(&computed, 3, target, normals.stride as usize, normals.offset as usize)
    .map_err(|e| throw_str(&ctx, &format!("computeNormals: {e}")))
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
    target: f.get::<_, Option<u64>>("target")?,
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
  let o = elements_of(&ctx, &origin, "raycast")?;
  let d = elements_of(&ctx, &direction, "raycast")?;
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
  let v = elements_of(ctx, data, api)?;
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
  let m = elements_of(&ctx, &motion, "sweep")?;
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
  let m = elements_of(&ctx, &motion, "moveAndSlide")?;
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
  let v = elements_of(&ctx, &vector, "bindDirectionSlot")?;
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
  let p = elements_of(&ctx, &post, "bindTextureSlot")?;
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

/// Bind the node's weights register to row `row` of the rgba32f texture:
/// four weights per texel, the register padded with zeros to the row -
/// one whole-texture upload per texture per flush, like a palette.
fn bind_weights_slot(ctx: Ctx<'_>, id: u64, texture: u64, row: u32) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial_bind_weights_slot(id, texture, row)
    .map_err(|e| throw_str(&ctx, &format!("bindWeightsSlot: {e}")))
}

/// Remove the node's weights slot on `texture`, or every weights slot
/// without one; abandoned rows keep their last value.
fn unbind_weights_slot(ctx: Ctx<'_>, id: u64, texture: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial_unbind_weights_slot(id, texture.0)
    .map_err(|e| throw_str(&ctx, &format!("unbindWeightsSlot: {e}")))
}

/// Write the node's weights register (a Float32Array of any length).
fn set_weights(ctx: Ctx<'_>, id: u64, weights: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let w = elements_of(&ctx, &weights, "setWeights")?;
  super::gui(&ctx).alloy.spatial().set_weights(id, w).map_err(|e| throw_str(&ctx, &format!("setWeights: {e}")))
}

/// Write the weights register THROUGH a motion: `transition` given (a
/// node motion spec, the `all` vocabulary), the write animates on it
/// whatever the node declares; otherwise the declaration's `weights`
/// entry (or `all`) decides, and without one the write snaps like
/// setWeights. A started, retargeted or held write requests a frame.
fn write_weights<'js>(
  ctx: Ctx<'js>,
  id: u64,
  weights: TypedArray<'js, f32>,
  transition: OptArg<Value<'js>>,
) -> rquickjs::Result<()> {
  let w = elements_of(&ctx, &weights, "writeWeights")?;
  let motion = match &transition.0 {
    Some(v) if !v.is_null() && !v.is_undefined() => {
      let pv = super::tree::to_prop_value(v)?;
      Some(decode_node_motion("writeWeights: transition", &pv).map_err(|e| throw_str(&ctx, &e))?)
    }
    _ => None,
  };
  let st = super::gui(&ctx);
  let changed =
    st.alloy.spatial().write_weights(id, w, motion).map_err(|e| throw_str(&ctx, &format!("writeWeights: {e}")))?;
  if changed {
    st.platform.request_frame();
  }
  Ok(())
}

/// Fill `out` from the node's weights register as the arena holds it
/// (zeros past the register's length); returns the register's length.
fn read_weights(ctx: Ctx<'_>, id: u64, out: TypedArray<'_, f32>) -> rquickjs::Result<u32> {
  let gui = super::gui(&ctx);
  let spatial = gui.alloy.spatial();
  let weights = spatial.weights_of(id).map_err(|e| throw_str(&ctx, &format!("readWeights: {e}")))?;
  let dst = elements_mut_of(&ctx, &out, "readWeights")?;
  for (k, slot) in dst.iter_mut().enumerate() {
    *slot = weights.get(k).copied().unwrap_or(0.0);
  }
  Ok(weights.len() as u32)
}

// Meta words per channel in createClip's packed layout.
const CLIP_META_WORDS: usize = 5;
/// Floats per joint in createRig's rest payload: position xyz, quaternion
/// xyzw, scale xyz.
const REST_FLOATS: usize = 10;

/// Register a baked clip: `meta` is [targetSlot, path (0 position,
/// 1 rotation, 2 scale, 3 weights, 4 curve), interpolation (0 step,
/// 1 linear, 2 cubic), keyCount, elements] per channel; `times` and
/// `values` are every channel's key arrays concatenated in meta order;
/// `curves` names the curve channels' slots. One crossing per clip.
fn create_clip<'js>(
  ctx: Ctx<'js>,
  duration: f64,
  meta: TypedArray<'js, u32>,
  times: TypedArray<'js, f32>,
  values: TypedArray<'js, f32>,
  curves: OptArg<Vec<String>>,
) -> rquickjs::Result<u64> {
  let meta = elements_of(&ctx, &meta, "createClip")?;
  let times = elements_of(&ctx, &times, "createClip")?;
  let values = elements_of(&ctx, &values, "createClip")?;
  if meta.len() % CLIP_META_WORDS != 0 {
    return Err(throw_str(&ctx, "createClip: meta must be 5 words per channel"));
  }
  let mut channels = Vec::with_capacity(meta.len() / CLIP_META_WORDS);
  let mut t_at = 0usize;
  let mut v_at = 0usize;
  for (i, entry) in meta.chunks(CLIP_META_WORDS).enumerate() {
    let path = match entry[1] {
      0 => ChannelPath::Position,
      1 => ChannelPath::Rotation,
      2 => ChannelPath::Scale,
      3 => ChannelPath::Weights,
      4 => ChannelPath::Curve,
      other => return Err(throw_str(&ctx, &format!("createClip: channel {i} path {other} is not 0 to 4"))),
    };
    let interpolation = match entry[2] {
      0 => ChannelInterpolation::Step,
      1 => ChannelInterpolation::Linear,
      2 => ChannelInterpolation::Cubic,
      other => return Err(throw_str(&ctx, &format!("createClip: channel {i} interpolation {other} is not 0, 1 or 2"))),
    };
    let keys = entry[3] as usize;
    // The element count is the core's to validate against the path.
    let elements = entry[4] as usize;
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
      elements,
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
    .create_clip(duration, channels, curves.0.unwrap_or_default())
    .map_err(|e| throw_str(&ctx, &format!("createClip: {e}")))
}

fn destroy_clip(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_clip(id).map_err(|e| throw_str(&ctx, &format!("destroyClip: {e}")))
}

/// Replace a clip's markers: `times` and `durations` (seconds, a
/// Float32Array each, 0 duration for an instant) with `names` alongside.
fn set_clip_markers<'js>(
  ctx: Ctx<'js>,
  clip: u64,
  times: TypedArray<'js, f32>,
  durations: TypedArray<'js, f32>,
  names: Vec<String>,
) -> rquickjs::Result<()> {
  let times = elements_of(&ctx, &times, "setClipMarkers")?;
  let durations = elements_of(&ctx, &durations, "setClipMarkers")?;
  if times.len() != durations.len() || times.len() != names.len() {
    return Err(throw_str(&ctx, "setClipMarkers: times, durations and names must be the same length"));
  }
  let markers = times
    .iter()
    .zip(durations.iter())
    .zip(names)
    .map(|((&time, &duration), name)| ClipMarker { time, duration, name })
    .collect();
  super::gui(&ctx)
    .alloy
    .spatial()
    .set_clip_markers(clip, markers)
    .map_err(|e| throw_str(&ctx, &format!("setClipMarkers: {e}")))
}

/// The additive form of a clip against a reference: `{ kind: "first" }`
/// (the clip's own first frame), `{ kind: "clip", clip, time }` or
/// `{ kind: "rest", rig }`. Returns a new clip.
fn derive_additive_clip<'js>(ctx: Ctx<'js>, clip: u64, reference: Object<'js>) -> rquickjs::Result<u64> {
  let kind: String = reference.get("kind")?;
  let reference = match kind.as_str() {
    "first" => AdditiveReference::FirstFrame,
    "clip" => {
      AdditiveReference::Clip { clip: reference.get::<_, u64>("clip")?, time: reference.get::<_, f64>("time")? as f32 }
    }
    "rest" => AdditiveReference::Rest { rig: reference.get::<_, u64>("rig")? },
    other => {
      return Err(throw_str(&ctx, &format!("deriveAdditiveClip: reference kind '{other}' is not first, clip or rest")))
    }
  };
  super::gui(&ctx)
    .alloy
    .spatial()
    .derive_additive(clip, reference)
    .map_err(|e| throw_str(&ctx, &format!("deriveAdditiveClip: {e}")))
}

/// Create a rig over `joints` clocked on `clock`; the layers compose
/// over each joint's rest pose (setRest, else the pose as it stands).
fn create_rig(ctx: Ctx<'_>, clock: u64, joints: Vec<u64>) -> rquickjs::Result<u64> {
  let st = super::gui(&ctx);
  let result = st.alloy.spatial().create_rig(clock, joints).map_err(|e| throw_str(&ctx, &format!("createRig: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

/// Declare a node's rest pose: ten floats (position xyz, quaternion
/// xyzw, scale xyz), or null to clear it.
fn set_rest<'js>(ctx: Ctx<'js>, id: u64, rest: OptArg<TypedArray<'js, f32>>) -> rquickjs::Result<()> {
  let pose = match rest.0 {
    None => None,
    Some(array) => {
      let r = elements_of(&ctx, &array, "setRest")?;
      if r.len() != REST_FLOATS {
        return Err(throw_str(&ctx, &format!("setRest: rest must hold {REST_FLOATS} floats")));
      }
      Some(RestPose { position: [r[0], r[1], r[2]], rotation: [r[3], r[4], r[5], r[6]], scale: [r[7], r[8], r[9]] })
    }
  };
  super::gui(&ctx).alloy.spatial().set_rest(id, pose).map_err(|e| throw_str(&ctx, &format!("setRest: {e}")))
}

/// Route a rig's curve into scalar param `name` of draw entry `draw` on
/// `target` (no draw: a target-level param), written at every flush the
/// value changed.
fn bind_curve_param(
  ctx: Ctx<'_>,
  rig: u64,
  curve: String,
  target: u64,
  name: String,
  draw: OptArg<u64>,
) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .bind_curve_param(rig, &curve, target, draw.0, name)
    .map_err(|e| throw_str(&ctx, &format!("bindCurveParam: {e}")))
}

fn unbind_curve_param(ctx: Ctx<'_>, rig: u64, curve: String, target: u64, draw: OptArg<u64>) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .unbind_curve_param(rig, &curve, target, draw.0)
    .map_err(|e| throw_str(&ctx, &format!("unbindCurveParam: {e}")))
}

fn destroy_rig(ctx: Ctx<'_>, rig: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_rig(rig).map_err(|e| throw_str(&ctx, &format!("destroyRig: {e}")))
}

fn layer_mode(ctx: &Ctx<'_>, mode: Option<String>, api: &str) -> rquickjs::Result<LayerMode> {
  match mode.as_deref() {
    None | Some("override") => Ok(LayerMode::Override),
    Some("additive") => Ok(LayerMode::Additive),
    Some(other) => Err(throw_str(ctx, &format!("{api}: mode '{other}' is not override or additive"))),
  }
}

/// A `mask` field: absent = untouched, null = none, a number = a mask id.
fn mask_field<'js>(ctx: &Ctx<'js>, o: &Object<'js>, api: &str) -> rquickjs::Result<Option<Option<u64>>> {
  let v: Value<'js> = o.get("mask")?;
  if v.is_undefined() {
    return Ok(None);
  }
  if v.is_null() {
    return Ok(Some(None));
  }
  match v.as_number() {
    Some(n) => Ok(Some(Some(n as u64))),
    None => Err(throw_str(ctx, &format!("{api}: mask must be a mask id or null"))),
  }
}

/// Append a layer: `opts` is `{ weight?, mode?: "override" | "additive",
/// mask? }` (defaults 1, override, none). Returns its index.
fn create_layer<'js>(ctx: Ctx<'js>, rig: u64, opts: OptArg<Object<'js>>) -> rquickjs::Result<u32> {
  let mut spec = LayerSpec { weight: 1.0, mode: LayerMode::Override, mask: None };
  if let Some(o) = opts.0 {
    if let Some(v) = o.get::<_, Option<f64>>("weight")? {
      spec.weight = v as f32;
    }
    spec.mode = layer_mode(&ctx, o.get::<_, Option<String>>("mode")?, "createLayer")?;
    spec.mask = mask_field(&ctx, &o, "createLayer")?.flatten();
  }
  let st = super::gui(&ctx);
  let result = st
    .alloy
    .spatial()
    .create_layer(rig, spec)
    .map(|i| i as u32)
    .map_err(|e| throw_str(&ctx, &format!("createLayer: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

/// Write a layer's `{ weight?, fadeMs?, mask? }` (mask null clears).
fn set_layer<'js>(ctx: Ctx<'js>, rig: u64, index: u32, update: Object<'js>) -> rquickjs::Result<()> {
  let update = LayerUpdate {
    weight: update.get::<_, Option<f64>>("weight")?.map(|v| v as f32),
    fade_ms: update.get::<_, Option<f64>>("fadeMs")?.unwrap_or(0.0) as f32,
    mask: mask_field(&ctx, &update, "setLayer")?,
  };
  let st = super::gui(&ctx);
  st.alloy.spatial().set_layer(rig, index as usize, update).map_err(|e| throw_str(&ctx, &format!("setLayer: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

fn destroy_layer(ctx: Ctx<'_>, rig: u64, index: u32) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  let result = st
    .alloy
    .spatial()
    .destroy_layer(rig, index as usize)
    .map_err(|e| throw_str(&ctx, &format!("destroyLayer: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

fn mask_shares<'js>(
  ctx: &Ctx<'js>,
  nodes: Vec<u64>,
  shares: &TypedArray<'js, f32>,
  api: &str,
) -> rquickjs::Result<Vec<(u64, f32)>> {
  let shares = elements_of(ctx, shares, api)?;
  if shares.len() != nodes.len() {
    return Err(throw_str(ctx, &format!("{api}: one share per node")));
  }
  Ok(nodes.into_iter().zip(shares.iter().copied()).collect())
}

/// Register a mask: `default` for every node not listed, `shares[i]` for
/// `nodes[i]`.
fn create_mask<'js>(
  ctx: Ctx<'js>,
  default: f64,
  nodes: Vec<u64>,
  shares: TypedArray<'js, f32>,
) -> rquickjs::Result<u64> {
  let shares = mask_shares(&ctx, nodes, &shares, "createMask")?;
  super::gui(&ctx)
    .alloy
    .spatial()
    .create_mask(default as f32, shares)
    .map_err(|e| throw_str(&ctx, &format!("createMask: {e}")))
}

fn set_mask<'js>(
  ctx: Ctx<'js>,
  id: u64,
  default: f64,
  nodes: Vec<u64>,
  shares: TypedArray<'js, f32>,
) -> rquickjs::Result<()> {
  let shares = mask_shares(&ctx, nodes, &shares, "setMask")?;
  let st = super::gui(&ctx);
  st.alloy.spatial().set_mask(id, default as f32, shares).map_err(|e| throw_str(&ctx, &format!("setMask: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

fn destroy_mask(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx).alloy.spatial().destroy_mask(id).map_err(|e| throw_str(&ctx, &format!("destroyMask: {e}")))
}

/// A `window` field: absent = untouched, null = the whole clip,
/// `[start, end]` = a subclip.
fn window_field<'js>(ctx: &Ctx<'js>, o: &Object<'js>, api: &str) -> rquickjs::Result<Option<Option<(f64, f64)>>> {
  let v: Value<'js> = o.get("window")?;
  if v.is_undefined() {
    return Ok(None);
  }
  if v.is_null() {
    return Ok(Some(None));
  }
  let pair: Vec<f64> =
    Vec::from_js(ctx, v).map_err(|_| throw_str(ctx, &format!("{api}: window must be [start, end] or null")))?;
  if pair.len() != 2 {
    return Err(throw_str(ctx, &format!("{api}: window must be [start, end] or null")));
  }
  Ok(Some(Some((pair[0], pair[1]))))
}

/// Start a player on `clip` on `rig`; `opts` is `{ layer?, speed?, loop?,
/// weight?, fade?, mask?, window?, fadeOut?, time? }` (the setPlayer
/// field names; defaults 0, 1, true, 1, 0, none, none, 0, 0).
fn create_player<'js>(ctx: Ctx<'js>, rig: u64, clip: u64, opts: OptArg<Object<'js>>) -> rquickjs::Result<u64> {
  let mut spec = PlayerSpec::default();
  if let Some(o) = opts.0 {
    if let Some(v) = o.get::<_, Option<f64>>("layer")? {
      spec.layer = v as usize;
    }
    if let Some(v) = o.get::<_, Option<f64>>("speed")? {
      spec.speed = v as f32;
    }
    if let Some(v) = o.get::<_, Option<bool>>("loop")? {
      spec.looped = v;
    }
    if let Some(v) = o.get::<_, Option<f64>>("weight")? {
      spec.weight = v as f32;
    }
    if let Some(v) = o.get::<_, Option<f64>>("fade")? {
      spec.fade = v as f32;
    }
    if let Some(v) = o.get::<_, Option<f64>>("fadeOut")? {
      spec.fade_out = v as f32;
    }
    if let Some(v) = o.get::<_, Option<f64>>("time")? {
      spec.time = v;
    }
    spec.mask = mask_field(&ctx, &o, "createPlayer")?.flatten();
    spec.window = window_field(&ctx, &o, "createPlayer")?.flatten();
  }
  let st = super::gui(&ctx);
  let result =
    st.alloy.spatial().create_player(rig, clip, spec).map_err(|e| throw_str(&ctx, &format!("createPlayer: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

/// Write the given fields of a player: { weight?, fade?, speed?, time?,
/// window?, mask? }. Setting time re-arms a finished player's end report
/// and returns the window changes the seek made as [marker, phase, ...]
/// pairs (phase 1 begin, 2 end).
fn set_player<'js>(ctx: Ctx<'js>, id: u64, value: Object<'js>) -> rquickjs::Result<Vec<u32>> {
  let update = PlayerUpdate {
    weight: value.get::<_, Option<f64>>("weight")?.map(|v| v as f32),
    fade: value.get::<_, Option<f64>>("fade")?.map(|v| v as f32),
    speed: value.get::<_, Option<f64>>("speed")?.map(|v| v as f32),
    time: value.get::<_, Option<f64>>("time")?,
    window: window_field(&ctx, &value, "setPlayer")?,
    mask: mask_field(&ctx, &value, "setPlayer")?,
  };
  let st = super::gui(&ctx);
  let changes = st.alloy.spatial().set_player(id, update).map_err(|e| throw_str(&ctx, &format!("setPlayer: {e}")))?;
  st.platform.request_frame();
  Ok(changes.iter().flat_map(|&(m, phase)| [m as u32, marker_phase_code(phase)]).collect())
}

/// The wire code of a marker phase (0 instant, 1 begin, 2 end).
fn marker_phase_code(phase: MarkerPhase) -> u32 {
  match phase {
    MarkerPhase::Instant => 0,
    MarkerPhase::Begin => 1,
    MarkerPhase::End => 2,
  }
}

/// A player's current time (seconds) and weight: `[time, weight]`.
fn read_player(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<Vec<f64>> {
  let st = super::gui(&ctx);
  let time = st.alloy.spatial().player_time(id).map_err(|e| throw_str(&ctx, &format!("readPlayer: {e}")))?;
  let weight = st.alloy.spatial().player_weight(id).map_err(|e| throw_str(&ctx, &format!("readPlayer: {e}")))?;
  Ok(vec![time, weight as f64])
}

/// Whether a player is inside marker `marker`'s window.
fn marker_inside(ctx: Ctx<'_>, id: u64, marker: u32) -> rquickjs::Result<bool> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .marker_inside(id, marker as usize)
    .map_err(|e| throw_str(&ctx, &format!("markerInside: {e}")))
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

/// Remove a player at once; returns the markers whose windows it was
/// inside (they end with it).
fn destroy_player(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<Vec<u32>> {
  let st = super::gui(&ctx);
  let ended = st.alloy.spatial().destroy_player(id);
  st.platform.request_frame();
  Ok(ended.into_iter().map(|m| m as u32).collect())
}

fn sync_mode(ctx: &Ctx<'_>, mode: Option<String>, api: &str) -> rquickjs::Result<SyncMode> {
  match mode.as_deref() {
    None | Some("time") => Ok(SyncMode::Time),
    Some("markers") => Ok(SyncMode::Markers),
    Some(other) => Err(throw_str(ctx, &format!("{api}: sync '{other}' is not time or markers"))),
  }
}

/// Make a sync group of `players` on `rig`, synced by `sync` ("time",
/// the default, or "markers").
fn create_group(ctx: Ctx<'_>, rig: u64, players: Vec<u64>, sync: OptArg<String>) -> rquickjs::Result<u64> {
  let mode = sync_mode(&ctx, sync.0, "createGroup")?;
  let st = super::gui(&ctx);
  let result =
    st.alloy.spatial().create_group(rig, players, mode).map_err(|e| throw_str(&ctx, &format!("createGroup: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

fn set_group(ctx: Ctx<'_>, id: u64, players: Vec<u64>) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  let result = st.alloy.spatial().set_group(id, players).map_err(|e| throw_str(&ctx, &format!("setGroup: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

fn destroy_group(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  let result = st.alloy.spatial().destroy_group(id).map_err(|e| throw_str(&ctx, &format!("destroyGroup: {e}")))?;
  st.platform.request_frame();
  Ok(result)
}

/// Request an inertial transition on a rig over `ms`.
fn inertialize(ctx: Ctx<'_>, rig: u64, ms: f64) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  st.alloy.spatial().inertialize(rig, ms as f32).map_err(|e| throw_str(&ctx, &format!("inertialize: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

/// A rig's curve as of the last advance; undefined for a curve no clip
/// fed this frame.
fn read_curve(ctx: Ctx<'_>, rig: u64, name: String) -> rquickjs::Result<Option<f64>> {
  super::gui(&ctx)
    .alloy
    .spatial()
    .curve_value(rig, &name)
    .map(|v| v.map(|v| v as f64))
    .map_err(|e| throw_str(&ctx, &format!("readCurve: {e}")))
}

/// Fill `out` (a Float32Array of 10) with the node's current local TRS
/// (position, quaternion, scale) - what the players last wrote, or any
/// later snap. The pose read for root-motion strips and skeleton copies.
fn read_transform(ctx: Ctx<'_>, id: u64, out: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let (p, q, s) =
    super::gui(&ctx).alloy.spatial().transform_of(id).map_err(|e| throw_str(&ctx, &format!("readTransform: {e}")))?;
  let dst = elements_mut_of(&ctx, &out, "readTransform")?;
  if dst.len() != 10 {
    return Err(throw_str(&ctx, "readTransform: out must be a Float32Array of 10"));
  }
  dst[0..3].copy_from_slice(&p);
  dst[3..7].copy_from_slice(&q);
  dst[7..10].copy_from_slice(&s);
  Ok(())
}

/// Declare a spring-bone chain (see `alloy::spatial::springs`): joint i is
/// `jointNodes[i]` with its 14 floats in `joints` - tail xyz, rest
/// rotation xyzw, stiffness, drag, gravity power, gravity direction xyz,
/// hit radius; collider i is `colliderNodes[i]` with its 9 floats in
/// `colliders` - kind (0 sphere, 1 capsule, 2 plane), inside (0 or 1),
/// two points xyz (the sphere's offset and nothing; the capsule's ends;
/// the plane's offset and normal) and radius.
fn create_spring<'js>(
  ctx: Ctx<'js>,
  clock: u64,
  joint_nodes: Vec<u64>,
  joints: TypedArray<'js, f32>,
  collider_nodes: Vec<u64>,
  colliders: TypedArray<'js, f32>,
  center: OptArg<u64>,
) -> rquickjs::Result<u64> {
  let jf = elements_of(&ctx, &joints, "createSpring")?;
  if jf.len() != joint_nodes.len() * SPRING_JOINT_FLOATS {
    return Err(throw_str(
      &ctx,
      &format!(
        "createSpring: joints must hold {SPRING_JOINT_FLOATS} floats per joint node ({} nodes, {} floats)",
        joint_nodes.len(),
        jf.len()
      ),
    ));
  }
  let joint_specs: Vec<SpringJoint> = joint_nodes
    .iter()
    .zip(jf.chunks_exact(SPRING_JOINT_FLOATS))
    .map(|(&node, f)| SpringJoint {
      node,
      tail: [f[0], f[1], f[2]],
      stiffness: f[3],
      drag: f[4],
      gravity_power: f[5],
      gravity_dir: [f[6], f[7], f[8]],
      hit_radius: f[9],
    })
    .collect();
  let cf = elements_of(&ctx, &colliders, "createSpring")?;
  if cf.len() != collider_nodes.len() * SPRING_COLLIDER_FLOATS {
    return Err(throw_str(
      &ctx,
      &format!(
        "createSpring: colliders must hold {SPRING_COLLIDER_FLOATS} floats per collider node ({} nodes, {} floats)",
        collider_nodes.len(),
        cf.len()
      ),
    ));
  }
  let mut collider_specs = Vec::with_capacity(collider_nodes.len());
  for (i, (&node, f)) in collider_nodes.iter().zip(cf.chunks_exact(SPRING_COLLIDER_FLOATS)).enumerate() {
    let inside = f[1] != 0.0;
    let (p0, p1, radius) = ([f[2], f[3], f[4]], [f[5], f[6], f[7]], f[8]);
    let shape = match f[0] as i32 {
      0 => SpringShape::Sphere { offset: p0, radius, inside },
      1 => SpringShape::Capsule { a: p0, b: p1, radius, inside },
      2 => SpringShape::Plane { offset: p0, normal: p1 },
      kind => return Err(throw_str(&ctx, &format!("createSpring: collider {i} has unknown kind {kind}"))),
    };
    collider_specs.push(SpringCollider { node, shape });
  }
  let st = super::gui(&ctx);
  let id = st
    .alloy
    .spatial()
    .create_spring(clock, joint_specs, collider_specs, center.0)
    .map_err(|e| throw_str(&ctx, &format!("createSpring: {e}")))?;
  // The rest pose is written at creation: a frame must publish it.
  st.platform.request_frame();
  Ok(id)
}

fn destroy_spring(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  // The joints go back to rest on the spot: a frame must publish it.
  if st.alloy.spatial().destroy_spring(id) {
    st.platform.request_frame();
  }
  Ok(())
}

fn reset_spring(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  st.alloy.spatial().reset_spring(id).map_err(|e| throw_str(&ctx, &format!("resetSpring: {e}")))?;
  // The rest pose is written on the spot: a frame must publish it.
  st.platform.request_frame();
  Ok(())
}

/// Change a joint's authored settings: the fields of `update` given
/// (`stiffness`, `drag`, `gravityPower`, `gravityDir` [x, y, z],
/// `hitRadius`) replace the joint's.
fn set_spring_joint<'js>(ctx: Ctx<'js>, id: u64, joint: u32, update: Object<'js>) -> rquickjs::Result<()> {
  let number =
    |key: &str| -> rquickjs::Result<Option<f32>> { Ok(update.get::<_, Option<f64>>(key)?.map(|v| v as f32)) };
  let gravity_dir = match update.get::<_, Option<Vec<f64>>>("gravityDir")? {
    None => None,
    Some(v) if v.len() == 3 => Some([v[0] as f32, v[1] as f32, v[2] as f32]),
    Some(_) => return Err(throw_str(&ctx, "setSpringJoint: gravityDir must be [x, y, z]")),
  };
  let update = SpringJointUpdate {
    stiffness: number("stiffness")?,
    drag: number("drag")?,
    gravity_power: number("gravityPower")?,
    gravity_dir,
    hit_radius: number("hitRadius")?,
  };
  let st = super::gui(&ctx);
  st.alloy
    .spatial()
    .set_spring_joint(id, joint as usize, update)
    .map_err(|e| throw_str(&ctx, &format!("setSpringJoint: {e}")))?;
  // New settings move the chain: a frame must step it.
  st.platform.request_frame();
  Ok(())
}

/// A required number of a spec object.
fn num_of(ctx: &Ctx<'_>, obj: &Object<'_>, key: &str, api: &str) -> rquickjs::Result<f32> {
  obj.get::<_, Option<f64>>(key)?.map(|v| v as f32).ok_or_else(|| throw_str(ctx, &format!("{api}: {key} is required")))
}

/// A required node id of a spec object.
fn node_of(ctx: &Ctx<'_>, obj: &Object<'_>, key: &str, api: &str) -> rquickjs::Result<u64> {
  obj.get::<_, Option<u64>>(key)?.ok_or_else(|| throw_str(ctx, &format!("{api}: {key} is required")))
}

/// An optional `[x, y, z]` of a spec object.
fn vec3_of(ctx: &Ctx<'_>, obj: &Object<'_>, key: &str, api: &str) -> rquickjs::Result<Option<[f32; 3]>> {
  match obj.get::<_, Option<Vec<f64>>>(key)? {
    None => Ok(None),
    Some(v) if v.len() == 3 => Ok(Some([v[0] as f32, v[1] as f32, v[2] as f32])),
    Some(_) => Err(throw_str(ctx, &format!("{api}: {key} must be [x, y, z]"))),
  }
}

fn vec3_req(ctx: &Ctx<'_>, obj: &Object<'_>, key: &str, api: &str) -> rquickjs::Result<[f32; 3]> {
  vec3_of(ctx, obj, key, api)?.ok_or_else(|| throw_str(ctx, &format!("{api}: {key} is required")))
}

/// An optional `[x, y, z, w]` of a spec object.
fn quat_of(ctx: &Ctx<'_>, obj: &Object<'_>, key: &str, api: &str) -> rquickjs::Result<Option<[f32; 4]>> {
  match obj.get::<_, Option<Vec<f64>>>(key)? {
    None => Ok(None),
    Some(v) if v.len() == 4 => Ok(Some([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])),
    Some(_) => Err(throw_str(ctx, &format!("{api}: {key} must be [x, y, z, w]"))),
  }
}

/// A point: `{ node }` or `{ position, space? }`.
fn point_of<'js>(ctx: &Ctx<'js>, obj: &Object<'js>, api: &str) -> rquickjs::Result<IkPoint> {
  if let Some(node) = obj.get::<_, Option<u64>>("node")? {
    return Ok(IkPoint::Node(node));
  }
  Ok(IkPoint::Point { position: vec3_req(ctx, obj, "position", api)?, space: obj.get::<_, Option<u64>>("space")? })
}

/// A target: null/undefined for none, `{ node }`, or `{ position,
/// rotation?, space? }`.
fn target_of<'js>(ctx: &Ctx<'js>, value: Value<'js>, api: &str) -> rquickjs::Result<Option<IkTarget>> {
  if value.is_null() || value.is_undefined() {
    return Ok(None);
  }
  let obj = Object::from_js(ctx, value).map_err(|_| throw_str(ctx, &format!("{api}: a target is an object")))?;
  if let Some(node) = obj.get::<_, Option<u64>>("node")? {
    return Ok(Some(IkTarget::Node(node)));
  }
  Ok(Some(IkTarget::Point {
    position: vec3_req(ctx, &obj, "position", api)?,
    rotation: quat_of(ctx, &obj, "rotation", api)?,
    space: obj.get::<_, Option<u64>>("space")?,
  }))
}

fn target_field<'js>(ctx: &Ctx<'js>, obj: &Object<'js>, api: &str) -> rquickjs::Result<Option<IkTarget>> {
  target_of(ctx, obj.get::<_, Value<'js>>("target")?, api)
}

fn point_field<'js>(ctx: &Ctx<'js>, obj: &Object<'js>, key: &str, api: &str) -> rquickjs::Result<Option<IkPoint>> {
  match obj.get::<_, Option<Object<'js>>>(key)? {
    None => Ok(None),
    Some(o) => point_of(ctx, &o, api).map(Some),
  }
}

/// A chain limit: `{ kind: "hinge", axis, min, max }` or `{ kind:
/// "cone", swing, twist }`.
fn limit_of<'js>(ctx: &Ctx<'js>, obj: &Object<'js>, api: &str) -> rquickjs::Result<IkLimit> {
  let kind: String = obj.get("kind")?;
  match kind.as_str() {
    "hinge" => Ok(IkLimit::Hinge {
      axis: vec3_req(ctx, obj, "axis", api)?,
      min: num_of(ctx, obj, "min", api)?,
      max: num_of(ctx, obj, "max", api)?,
    }),
    "cone" => Ok(IkLimit::Cone { swing: num_of(ctx, obj, "swing", api)?, twist: num_of(ctx, obj, "twist", api)? }),
    other => Err(throw_str(ctx, &format!("{api}: unknown limit kind {other:?}"))),
  }
}

/// One solver spec object (see `createIk` in spatial.d.ts) and its
/// weight.
fn solver_of<'js>(ctx: &Ctx<'js>, obj: &Object<'js>, api: &str) -> rquickjs::Result<(IkSolverSpec, f32)> {
  let kind: String = obj.get("kind")?;
  let weight = obj.get::<_, Option<f64>>("weight")?.unwrap_or(1.0) as f32;
  let spec = match kind.as_str() {
    "lookAt" => {
      let mut bones = Vec::new();
      for b in obj.get::<_, Array<'js>>("bones")?.iter::<Object<'js>>() {
        let b = b?;
        bones.push(LookAtBone { node: node_of(ctx, &b, "node", api)?, share: num_of(ctx, &b, "share", api)? });
      }
      let up = match obj.get::<_, Option<Object<'js>>>("up")? {
        None => None,
        Some(u) => Some(IkUp { vector: vec3_req(ctx, &u, "vector", api)?, space: u.get::<_, Option<u64>>("space")? }),
      };
      let limit = match obj.get::<_, Option<Object<'js>>>("limit")? {
        None => None,
        Some(l) => {
          let pair = |key: &str| -> rquickjs::Result<[f32; 2]> {
            match l.get::<_, Option<Vec<f64>>>(key)? {
              Some(v) if v.len() == 2 => Ok([v[0] as f32, v[1] as f32]),
              _ => Err(throw_str(ctx, &format!("{api}: limit.{key} must be [min, max]"))),
            }
          };
          Some(LookAtLimit { yaw: pair("yaw")?, pitch: pair("pitch")?, damp: num_of(ctx, &l, "damp", api)? })
        }
      };
      IkSolverSpec::LookAt(LookAtSpec {
        bones,
        target: target_field(ctx, obj, api)?,
        forward: vec3_req(ctx, obj, "forward", api)?,
        up_axis: vec3_req(ctx, obj, "upAxis", api)?,
        up,
        limit,
        ease_ms: num_of(ctx, obj, "easeMs", api)?,
      })
    }
    "twoBone" => IkSolverSpec::TwoBone(TwoBoneSpec {
      root: node_of(ctx, obj, "root", api)?,
      mid: node_of(ctx, obj, "mid", api)?,
      tip: node_of(ctx, obj, "tip", api)?,
      target: target_field(ctx, obj, api)?,
      pole: point_field(ctx, obj, "pole", api)?,
      pole_angle: num_of(ctx, obj, "poleAngle", api)?,
      position_weight: num_of(ctx, obj, "positionWeight", api)?,
      rotation_weight: num_of(ctx, obj, "rotationWeight", api)?,
      soft: num_of(ctx, obj, "soft", api)?,
      stretch: num_of(ctx, obj, "stretch", api)?,
    }),
    "chain" => {
      let method: String = obj.get("method")?;
      let method = match method.as_str() {
        "fabrik" => ChainMethod::Fabrik,
        "ccd" => ChainMethod::Ccd,
        other => return Err(throw_str(ctx, &format!("{api}: unknown chain method {other:?}"))),
      };
      let mut limits = Vec::new();
      if let Some(list) = obj.get::<_, Option<Array<'js>>>("limits")? {
        for l in list.iter::<Object<'js>>() {
          let l = l?;
          let limit: Object<'js> = l.get("limit")?;
          limits.push((node_of(ctx, &l, "node", api)?, limit_of(ctx, &limit, api)?));
        }
      }
      IkSolverSpec::Chain(ChainSpec {
        root: node_of(ctx, obj, "root", api)?,
        tip: node_of(ctx, obj, "tip", api)?,
        target: target_field(ctx, obj, api)?,
        method,
        iterations: num_of(ctx, obj, "iterations", api)?.max(0.0) as u32,
        tolerance: num_of(ctx, obj, "tolerance", api)?,
        magnet: point_field(ctx, obj, "magnet", api)?,
        limits,
        rotation_weight: num_of(ctx, obj, "rotationWeight", api)?,
      })
    }
    "feet" => {
      let mut legs = Vec::new();
      for l in obj.get::<_, Array<'js>>("legs")?.iter::<Object<'js>>() {
        let l = l?;
        legs.push(LegSpec {
          root: node_of(ctx, &l, "root", api)?,
          mid: node_of(ctx, &l, "mid", api)?,
          tip: node_of(ctx, &l, "tip", api)?,
          height: l.get::<_, Option<f64>>("height")?.map(|v| v as f32),
        });
      }
      IkSolverSpec::Feet(FeetSpec {
        hips: node_of(ctx, obj, "hips", api)?,
        legs,
        up: vec3_req(ctx, obj, "up", api)?,
        filter: filter_arg(OptArg(obj.get::<_, Option<Object<'js>>>("filter")?))?,
        rise: num_of(ctx, obj, "rise", api)?,
        drop: num_of(ctx, obj, "drop", api)?,
        plant_height: num_of(ctx, obj, "plantHeight", api)?,
        plant_band: num_of(ctx, obj, "plantBand", api)?,
        hips_ease_ms: num_of(ctx, obj, "hipsEaseMs", api)?,
        rotation_weight: num_of(ctx, obj, "rotationWeight", api)?,
      })
    }
    other => return Err(throw_str(ctx, &format!("{api}: unknown solver kind {other:?}"))),
  };
  Ok((spec, weight))
}

/// Declare an IK stack (see `alloy::spatial::ik`): the solver spec
/// objects in order, each with its `weight`.
fn create_ik<'js>(ctx: Ctx<'js>, clock: u64, solvers: Array<'js>) -> rquickjs::Result<u64> {
  let mut specs = Vec::with_capacity(solvers.len());
  for s in solvers.iter::<Object<'js>>() {
    specs.push(solver_of(&ctx, &s?, "createIk")?);
  }
  let st = super::gui(&ctx);
  let id = st.alloy.spatial().create_ik(clock, specs).map_err(|e| throw_str(&ctx, &format!("createIk: {e}")))?;
  // The first solve lands at the next post-pass: a frame must run it.
  st.platform.request_frame();
  Ok(id)
}

fn destroy_ik(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  // The joints go back to the primary pose on the spot: a frame must
  // publish it.
  if st.alloy.spatial().destroy_ik(id) {
    st.platform.request_frame();
  }
  Ok(())
}

/// Replace a solver's settings (a full spec object of the same kind).
fn set_ik_solver<'js>(ctx: Ctx<'js>, id: u64, index: u32, spec: Object<'js>) -> rquickjs::Result<()> {
  let (spec, _) = solver_of(&ctx, &spec, "setIkSolver")?;
  let st = super::gui(&ctx);
  st.alloy
    .spatial()
    .set_ik_solver(id, index as usize, spec)
    .map_err(|e| throw_str(&ctx, &format!("setIkSolver: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

/// Move a solver's target (null for none).
fn set_ik_target<'js>(ctx: Ctx<'js>, id: u64, index: u32, target: Value<'js>) -> rquickjs::Result<()> {
  let target = target_of(&ctx, target, "setIkTarget")?;
  let st = super::gui(&ctx);
  st.alloy
    .spatial()
    .set_ik_target(id, index as usize, target)
    .map_err(|e| throw_str(&ctx, &format!("setIkTarget: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

/// Ramp a solver's weight over `fadeMs` (0 or absent snaps).
fn set_ik_weight(ctx: Ctx<'_>, id: u64, index: u32, weight: f64, fade_ms: OptArg<f64>) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  st.alloy
    .spatial()
    .set_ik_weight(id, index as usize, weight as f32, fade_ms.0.unwrap_or(0.0) as f32)
    .map_err(|e| throw_str(&ctx, &format!("setIkWeight: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

/// Bind a solver's weight to a rig's curve (readCurve) from here on; a
/// setIkWeight unbinds.
fn bind_ik_weight_curve(ctx: Ctx<'_>, id: u64, index: u32, rig: u64, name: String) -> rquickjs::Result<()> {
  let st = super::gui(&ctx);
  st.alloy
    .spatial()
    .bind_ik_weight_curve(id, index as usize, rig, name)
    .map_err(|e| throw_str(&ctx, &format!("bindIkWeightCurve: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

/// The last solve's residual.
fn read_ik_reach(ctx: Ctx<'_>, id: u64, index: u32) -> rquickjs::Result<f64> {
  let st = super::gui(&ctx);
  let reach =
    st.alloy.spatial().ik_reach(id, index as usize).map_err(|e| throw_str(&ctx, &format!("readIkReach: {e}")))?;
  Ok(reach as f64)
}

/// Set the chain's runtime force (a Float32Array of 3, world space).
fn set_spring_force(ctx: Ctx<'_>, id: u64, force: TypedArray<'_, f32>) -> rquickjs::Result<()> {
  let f = elements_of(&ctx, &force, "setSpringForce")?;
  if f.len() != 3 {
    return Err(throw_str(&ctx, "setSpringForce: force must be a Float32Array of 3"));
  }
  let st = super::gui(&ctx);
  st.alloy
    .spatial()
    .set_spring_force(id, [f[0], f[1], f[2]])
    .map_err(|e| throw_str(&ctx, &format!("setSpringForce: {e}")))?;
  st.platform.request_frame();
  Ok(())
}

/// Bind the node's instance-record sink with the 2D pose projection: the
/// flush writes [x, y, angle, sx, sy] to record slot `index` of vertex
/// buffer `buffer`, batched into one buffer write per flush.
fn bind_pose_record(ctx: Ctx<'_>, id: u64, buffer: u64, index: u32) -> rquickjs::Result<()> {
  let sink = InstanceRecordSink { buffer, index, projection: InstanceProjection::Pose2D };
  super::gui(&ctx)
    .alloy
    .spatial_bind_records(id, vec![sink], None)
    .map_err(|e| throw_str(&ctx, &format!("bindPoseRecord: {e}")))
}

/// Bind the node's instance-record sink with the matrix projection: the
/// flush writes the node's world matrix, relative to `anchor` when given,
/// as 16 floats to record slot `index` of vertex buffer `buffer`.
fn bind_matrix_record(
  ctx: Ctx<'_>,
  id: u64,
  buffers: Vec<u64>,
  index: u32,
  anchor: OptArg<u64>,
) -> rquickjs::Result<()> {
  if buffers.is_empty() {
    return Err(throw_str(&ctx, "bindMatrixRecord: buffers must name at least one buffer"));
  }
  let sinks = buffers
    .into_iter()
    .map(|buffer| InstanceRecordSink { buffer, index, projection: InstanceProjection::Matrix })
    .collect();
  super::gui(&ctx)
    .alloy
    .spatial_bind_records(id, sinks, anchor.0)
    .map_err(|e| throw_str(&ctx, &format!("bindMatrixRecord: {e}")))
}

fn unbind_record(ctx: Ctx<'_>, id: u64) -> rquickjs::Result<()> {
  super::gui(&ctx)
    .alloy
    .spatial_bind_records(id, Vec::new(), None)
    .map_err(|e| throw_str(&ctx, &format!("unbindRecord: {e}")))
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

/// What a frame's draw-side node-transition tick produced (see `tick`).
pub(crate) struct SpatialTick {
  /// Motion still runs after the starts and the free gates: the runner's
  /// signal to keep requesting frames (the step's own signal is
  /// `advance_transitions`' return, carried over the frame by the frame
  /// module).
  pub active: bool,
  /// The flush sent sink writes: this frame must paint.
  pub wrote: bool,
  /// The residency gate owes a delivery (region events waiting) or has
  /// enters still to pace: the next frame must run.
  pub regions: bool,
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
/// path's flush publishes the result. What the advance has to tell JS
/// (finished and dropped players, root-motion deltas) waits in the arena
/// for `deliver_motion`.
pub(crate) fn advance_players(ctx: &Ctx<'_>) -> PlayersTick {
  let Some(st) = super::try_gui(ctx) else {
    return PlayersTick { active: false, wrote: false };
  };
  let tick = st.alloy.spatial().advance_players();
  PlayersTick { active: tick.active, wrote: tick.wrote }
}

/// Step the node transitions to the stamped clock: every running track
/// writes its node's TRS through the arena's ordinary snap path (the
/// draw tick's flush publishes it). The frame module calls this BEFORE
/// the frame's JS, right after the players, so the frame reads the
/// stepped pose (a late-pass follow, a shadow camera placed from a
/// light's world matrix) and may overwrite it. The settles wait in the
/// arena for `deliver_motion`. Returns whether any track still runs or
/// any write still waits on a node whose time moves: standing demand the
/// frame module carries to `frame::draw`.
pub(crate) fn advance_transitions(ctx: &Ctx<'_>) -> bool {
  let Some(st) = super::try_gui(ctx) else {
    return false;
  };
  let active = st.alloy.spatial().advance_transitions();
  active
}

/// Hand JS what the native motion of this frame's advance produced, as
/// engine events: one "spatialClipEnd" per finished or dropped player
/// (`{ player, reason }`), one "spatialClipLoop" per wrap (`{ player }`),
/// one "spatialClipMarker" per marker crossing (`{ player, marker, phase,
/// weight, layer }`), one "spatialRigEnd" per rig whose clock or joint
/// died (`{ rig }`), one
/// "spatialRootMotion" per bound player's delta, payload `{ player, x,
/// y, z, yaw }`, and one "spatialTransitionEnd" per settled track,
/// payload `{ node, component }`, then the residency gate's changes: one
/// "spatialRegionEnter" or "spatialRegionLeave" per region whose wanted
/// state flipped, payload `{ node }` for an explicit region (setRegion) and
/// `{ grid, cell: [i, j, k] }` for a grid cell (createGrid), the leaves of
/// a pass before its enters. The frame module calls this in `deliver`,
/// after the frame's clock reached JS ("frameStart") and ahead of the
/// frame's callbacks, so a handler sees the frame's stepped poses and its
/// clock, and its follow-up write (a region's creates) lands in the same
/// frame.
pub(crate) fn deliver_motion(ctx: &Ctx<'_>) {
  let Some(st) = super::try_gui(ctx) else {
    return;
  };
  // Each list is taken before its loop: a handler reads the arena (a
  // pose, a world matrix), so the borrow must not outlive the take.
  let clips = st.alloy.spatial().take_clip_events();
  let root_motion = st.alloy.spatial().take_root_motion();
  let settled = st.alloy.spatial().take_settled_transitions();
  let regions = st.alloy.spatial().take_region_events();
  for event in clips {
    let obj = Object::new(ctx.clone()).expect("create spatial clip event object");
    match event {
      ClipEvent::Finished(id) | ClipEvent::Dropped(id) => {
        let reason = if matches!(event, ClipEvent::Finished(_)) { "finished" } else { "dropped" };
        obj.set("player", id).expect("set player");
        obj.set("reason", reason).expect("set reason");
        crate::emit_event(ctx, "spatialClipEnd", obj);
      }
      ClipEvent::Loop(id) => {
        obj.set("player", id).expect("set player");
        crate::emit_event(ctx, "spatialClipLoop", obj);
      }
      ClipEvent::Marker { player, marker, phase, weight, layer } => {
        let phase = match phase {
          MarkerPhase::Instant => "instant",
          MarkerPhase::Begin => "begin",
          MarkerPhase::End => "end",
        };
        obj.set("player", player).expect("set player");
        obj.set("marker", marker as u32).expect("set marker");
        obj.set("phase", phase).expect("set phase");
        obj.set("weight", weight as f64).expect("set weight");
        obj.set("layer", layer as u32).expect("set layer");
        crate::emit_event(ctx, "spatialClipMarker", obj);
      }
      ClipEvent::RigDropped(rig) => {
        obj.set("rig", rig).expect("set rig");
        crate::emit_event(ctx, "spatialRigEnd", obj);
      }
    }
  }
  for (player, delta, yaw) in root_motion {
    let obj = Object::new(ctx.clone()).expect("create spatialRootMotion object");
    obj.set("player", player).expect("set player");
    obj.set("x", delta[0] as f64).expect("set x");
    obj.set("y", delta[1] as f64).expect("set y");
    obj.set("z", delta[2] as f64).expect("set z");
    obj.set("yaw", yaw as f64).expect("set yaw");
    crate::emit_event(ctx, "spatialRootMotion", obj);
  }
  for (node, component) in settled {
    let obj = Object::new(ctx.clone()).expect("create spatialTransitionEnd object");
    obj.set("node", node).expect("set node");
    obj.set("component", component_name(component)).expect("set component");
    crate::emit_event(ctx, "spatialTransitionEnd", obj);
  }
  for event in regions {
    let (name, region) = match event {
      RegionEvent::Enter(r) => ("spatialRegionEnter", r),
      RegionEvent::Leave(r) => ("spatialRegionLeave", r),
    };
    let obj = Object::new(ctx.clone()).expect("create spatial region event object");
    match region {
      RegionRef::Node(node) => obj.set("node", node).expect("set node"),
      RegionRef::Cell { grid, cell } => {
        obj.set("grid", grid).expect("set grid");
        obj.set("cell", cell.to_vec()).expect("set cell");
      }
    }
    crate::emit_event(ctx, name, obj);
  }
}

/// The draw-side half of the node transitions, after the frame's JS:
/// start what that JS declared and the frame's own start pass missed
/// (`start_transitions`, for nodes created outside a frame), run the free
/// gates (`check_exits`), flush the arena, then emit one
/// "spatialNodeFreed" engine event per leaving node freed, payload
/// `{ node }`. The flush is unconditional: besides the step's and the
/// starts' writes, the queue may hold clip-player poses and whatever the
/// frame's JS wrote without its own microtask flush landing yet. An empty
/// queue is a cheap no-op. The freed events come after the flush, so the
/// corpses' slot-zeroing writes have landed and a consumer recycling a
/// record slot on the event never races the corpse's last frame.
/// `frame::draw` calls this beside the render tree's transition advance,
/// before the frame's demand gate.
pub(crate) fn tick(ctx: &Ctx<'_>) -> SpatialTick {
  let Some(st) = super::try_gui(ctx) else {
    return SpatialTick { active: false, wrote: false, regions: false };
  };
  st.alloy.spatial().start_transitions();
  let active = st.alloy.spatial().check_exits();
  // The pose post-pass: the IK stacks bend the frame's final primary
  // pose (players, transitions and the frame's own writes) toward the
  // world, the spring chains step over the result, and the flush below
  // publishes their joints.
  let ik = st.alloy.spatial().advance_ik();
  let springs = st.alloy.spatial().advance_springs();
  let active = active || ik.active || springs.active;
  let freed = st.alloy.spatial().take_freed();
  let wrote = st.alloy.spatial_flush();
  let regions = st.alloy.spatial().residency_demand();
  for node in freed {
    let obj = Object::new(ctx.clone()).expect("create spatialNodeFreed object");
    obj.set("node", node).expect("set node");
    crate::emit_event(ctx, "spatialNodeFreed", obj);
  }
  SpatialTick { active, wrote, regions }
}
