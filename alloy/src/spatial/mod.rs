// The spatial core: a transform hierarchy whose flush recomputes only what
// changed and hands fresh world matrices to sinks. Generic on purpose - no
// camera, no mesh, no lights - so any consumer with a tree of transforms
// (the 3d package first, a 2D sprite scene or a skeleton just as well) gets
// the interpreter-hostile part of a scene graph in native code. Lives on
// the main thread beside `Context`, which turns sink writes into raster
// commands; the raster thread is untouched. Engine independent: native
// types in and out, no scripting references (rendertree rules).
//
// Cost model: a write marks one node and queues it; the flush recomputes
// the subtrees under the queued nodes that have no queued ancestor, so a
// moved node costs its own subtree and nothing else, however big the tree.
// Node ids carry a generation and are never reused.

mod bvh;
mod collide;
mod cull;
mod math;
mod mover;
mod normals;
mod pick;
mod players;
mod transitions;

use std::collections::HashMap;

pub use bvh::{ray_box_distance, Box3};
pub use collide::{Impact, Overlap, Volume};
pub use math::{compose, invert_affine, multiply, normal_matrix, transform_point, transform_vector, IDENTITY};
pub use mover::{MoveOptions, MoveResult};
pub use normals::{vertex_normals, write_channel};
pub use pick::{Hit, Shape, ShapeId};
// The per-triangle narrowphase, for tests: the brute-force oracle the
// indexed volume queries are checked against.
#[cfg(test)]
pub(crate) use collide::{segment_triangle, Query};
// The linear narrowphase and the indexing threshold, for tests: the
// brute-force path is the oracle the BVH path is checked against.
#[cfg(test)]
#[cfg(test)]
pub(crate) use pick::{ray_shape, BVH_MIN_TRIANGLES};
pub use players::{
  ChannelInterpolation, ChannelPath, ClipChannel, ClipEvent, ClipId, PlayerId, PlayerUpdate, PlayersTick, RootMotion,
};
// The pure sampler, for the differential tests.
#[cfg(test)]
pub(crate) use players::sample as sample_channel;
pub use transitions::{
  Component, Lanes, MotionState, NodeEndpoint, NodeMotion, NodeTransitionConfig, NodeTransitionEntry,
};

use bvh::Bvh;
use cull::{union, world_box, Frustum};
use transitions::{lanes3, targets_match, NodeTransitions, PendingWeights, PendingWrite, COMPONENTS};

pub type Mat4 = [f32; 16];

/// A stable node handle: arena index in the low 32 bits, generation in the
/// high 32 - a destroyed node's id never resolves again.
pub type NodeId = u64;

/// What one query (raycast, overlap, sweep, move_and_slide) admits, on top
/// of "shown with bounds": only nodes under `root` (the consumer's own
/// subtree in an arena shared by every scene and layer), whose `layers`
/// intersect the mask, and, with `nodes`, only those listed. The default
/// admits everything.
#[derive(Clone, Debug, Default)]
pub struct QueryFilter {
  pub root: Option<NodeId>,
  /// A layer mask; None admits every mask (a node's `layers` of 0 is then
  /// still admitted, so a caller that wants the mask rule passes one).
  pub layers: Option<u32>,
  pub nodes: Option<Vec<NodeId>>,
  /// A draw target: the query then sees the LOD levels that target draws
  /// (a node under a level the target has switched off is skipped like a
  /// hidden one). None sees every level.
  pub target: Option<u64>,
}

/// Where a node's fresh world matrix goes: the `uModel` (+ `uNormal`) params
/// of one draw entry, plus the entry's instance count as its visibility
/// switch (`count` is what "shown" restores: 1 for a plain mesh, the record
/// count for an instanced one). A node carries one draw sink PER TARGET
/// (a mesh drawn by the scene and by each of its views), all fed by the
/// same flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawSink {
  pub target: u64,
  pub draw: u64,
  pub normal: bool,
  pub count: u32,
  /// The entry's program takes `uLodFade`: the LOD pass writes the
  /// cross-fade band position there (see `write_fade`). Without it the
  /// entry switches hard at the band's midpoint.
  pub fade: bool,
  /// Where the entry sorts on a target ordering its entries
  /// (`set_draw_sort`); ignored elsewhere.
  pub order: DrawOrder,
}

/// The queue a draw entry sorts in on a sorted target, drawn in this
/// order: opaques front-to-back, then cutouts (an alpha-tested fragment
/// discards, which defeats early-z, so the solids fill the depth first)
/// front-to-back the same way, then transparents back-to-front.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawQueue {
  #[default]
  Opaque,
  Cutout,
  Transparent,
}

/// A draw entry's place in a sorted target (`Spatial::set_draw_sort`):
/// its queue above all, `render_order` (ascending) above the depth term
/// inside the queue, and equal keys keep bind order. Depth is measured
/// at the center of the node's world box, or its origin without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawOrder {
  pub queue: DrawQueue,
  pub render_order: i32,
}

/// A bound draw sink and its per-entry flush state.
#[derive(Clone, Copy)]
struct BoundSink {
  sink: DrawSink,
  /// Bind order across the tree: the sort's final tiebreak.
  seq: u64,
  /// The entry is switched on (instance count = `sink.count`).
  entry_on: bool,
  /// The entry owes a params write at the next shown flush: newly bound,
  /// or the node moved while hidden.
  fresh: bool,
  /// The `uLodFade` value last written (the solid `[1, 1]` until a band
  /// is entered; entries are created solid).
  fade: [f32; 2],
}

/// The consumer of sink writes, one method per write kind, called in flush
/// order. The core's entire output contract: everything a flush produces
/// goes through this trait, and the core never sees where it lands (alloy's
/// Context resolves the ids against its draw entries and forwards down the
/// raster channel; tests record). Arguments are borrowed from core state -
/// an implementation copies what it keeps.
///
/// Every write returns whether it landed. False means the resource is gone
/// (a destroyed target, buffer or texture; ids are never reused, so it is
/// gone for good), and the core releases the binding that produced the
/// write: the draw sink, or the slot/record/palette group. A dead binding
/// thus costs one write, not one per frame.
pub trait SinkWriter {
  /// A shown entry's fresh world transform: `uModel`, plus `uNormal` when
  /// the sink asked for it.
  fn write_params(&mut self, target: u64, draw: u64, model: &Mat4, normal: Option<&Mat4>) -> bool;
  /// An entry's instance count - the visibility switch (0 = hidden, the
  /// sink's count = shown).
  fn write_count(&mut self, target: u64, draw: u64, count: u32) -> bool;
  /// An entry's `uLodFade` (threshold, side): inside a LOD cross-fade
  /// band the nearer level keeps the fragments whose screen hash is below
  /// the threshold (side 1) and the farther level the rest (side -1);
  /// `[1, 1]` is solid. Written only for sinks bound with `fade`.
  fn write_fade(&mut self, target: u64, draw: u64, fade: [f32; 2]) -> bool;
  /// A sorted target's bound entries in draw order (`set_draw_sort`).
  /// Entries the core does not bind keep their places, so the writer
  /// composes the target's full permutation around these. False means
  /// the order did not land; nothing is released over it.
  fn write_order(&mut self, target: u64, order: &[u64]) -> bool;
  /// A shared-slot group's array param, rewritten whole (slot sinks share
  /// one array value; see `SharedSlotSink`).
  fn write_shared(&mut self, target: u64, name: &str, values: &[f32]) -> bool;
  /// One buffer's staged instance records: the coalesced dirty float range
  /// `[lo, hi)` plus `values`, the WHOLE staging mirror - so a writer that
  /// must publish the full record set (an ordered instance buffer gathers
  /// into draw order, where a partial range has no stable position) can
  /// reach every record, while the plain path writes just the range. At
  /// most one write per buffer per flush, however many nodes moved (see
  /// `InstanceRecordSink`).
  fn write_instances(&mut self, buffer: u64, lo: u32, hi: u32, values: &[f32]) -> bool;
  /// A float texture's fresh rows, whole: 16 floats (one column-major mat4,
  /// one row of a 4-texel-wide rgba32f texture) per bound slot. At most one
  /// write per texture per flush (see `TextureSlotSink`).
  fn write_texture(&mut self, texture: u64, values: &[f32]) -> bool;
}

/// How a shared-slot sink projects the node's world transform into its
/// three floats. `Direction` is `normalize(worldRotation * v)` (zeros for
/// a degenerate result); `Position` is the world translation - the pair a
/// positional light needs (a spot light feeds both arrays of one target).
#[derive(Clone, Debug, PartialEq)]
pub enum Projection {
  /// The world direction of this LOCAL vector.
  Direction([f32; 3]),
  /// The node's world position.
  Position,
}

/// Routes a projection of the node's world transform to one vec3 slot of
/// a target shared param: floats [index*3, index*3+3) of the `len`-float
/// array param `name`, shared by every sink naming it - the whole array
/// is one param value, re-sent when any slot changes, absent slots zero.
/// The generic form of "a scene's light directions follow the node tree":
/// the consumer picks the param name and packs non-spatial data (colors,
/// counts) itself - core never learns what the slots mean. A node carries
/// one slot sink per (target, param name), so one node may feed several
/// arrays of one target (a spot light: its direction and its position).
#[derive(Clone, Debug, PartialEq)]
pub struct SharedSlotSink {
  pub target: u64,
  pub name: String,
  /// Total floats of the array param (a multiple of 3).
  pub len: u32,
  /// vec3 slot index within it.
  pub index: u32,
  pub projection: Projection,
}

/// One shared-param array and the sinks feeding it.
struct SharedGroup {
  values: Vec<f32>,
  refs: u32,
  dirty: bool,
}

/// One level of a LOD group (`set_lod`): the node drawn at this level (a
/// direct child of the group), or None for a population level (the
/// instance nodes under the group carry one record sink per level and
/// pick among them), and `size` - the projected size (the group's
/// bounding sphere's diameter as a fraction of the viewport height) BELOW
/// which the level hands over to the next. Levels are listed nearest
/// first, sizes strictly descending; the last level's size is the cull
/// threshold, 0 for never culled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodLevel {
  pub node: Option<NodeId>,
  pub size: f32,
}

/// What a target measures projected size with (`set_lod_view`): the eye
/// position, the projection's vertical focal factor (`1 / tan(fov / 2)`
/// for a perspective projection, `2 / (top - bottom)` for an orthographic
/// one, `ortho` telling which), and a bias every measured size is
/// multiplied by (a quality knob: below 1 switches sooner).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodView {
  pub eye: [f32; 3],
  /// The unit view direction: what the draw sort measures transparent
  /// depth along (`set_draw_sort`).
  pub forward: [f32; 3],
  pub focal: f32,
  pub ortho: bool,
  pub bias: f32,
}

/// The one-sided hysteresis band of a hard LOD switch, as a fraction of
/// the level size: a level is left when the size falls below its
/// threshold and re-entered only once the size climbs above the threshold
/// times (1 + this), so a boundary never flickers frame to frame.
pub const LOD_HYSTERESIS: f32 = 0.1;

/// Below this eye distance the measured size is capped (a group at the
/// eye would otherwise divide by zero).
const LOD_MIN_DISTANCE: f32 = 1e-6;

/// Opaque entries sort by a logarithmic bucket of their center's distance
/// to the eye, this many buckets per doubling: early-z only needs near
/// layers drawn before far ones, and a coarse key keeps the order (and the
/// bind-order grouping within a bucket) stable while the camera moves.
const ORDER_BUCKETS_PER_DOUBLING: f32 = 4.0;
/// Distances at or below this share the nearest bucket (a center at the
/// eye); also keeps the log finite.
const ORDER_DISTANCE_FLOOR: f32 = 1e-3;
/// The bucket of ORDER_DISTANCE_FLOOR, so bucket terms start at 0.
const ORDER_BUCKET_BASE: i32 = -40;
/// The fraction of the nearest bucketed distance a camera may move without
/// any bucketed center leaving its bucket (1 - 2^(-1 / buckets per
/// doubling)): the bucket edge below a center is at most this close, so a
/// shorter move skips the keying.
const ORDER_BUCKET_MARGIN: f32 = 0.1591;
/// The render_order magnitude the packed key holds; beyond it values
/// clamp and tie.
const ORDER_RENDER_LIMIT: i32 = 1 << 20;

/// A sorted target's state between flushes (`Spatial::set_draw_sort`).
#[derive(Default)]
struct DrawSort {
  /// The bound entries in the order last written; empty before the first.
  last: Vec<u64>,
  /// The eye and forward the last keying measured from, and the nearest
  /// bucketed (opaque or cutout) center's distance to the eye then.
  eye: [f32; 3],
  forward: [f32; 3],
  nearest: f32,
  /// A transparent entry was among the keyed: exact depth, so every view
  /// move re-keys.
  transparent: bool,
  /// A bound entry changed (bound, unbound, re-keyed, or its node
  /// touched): re-key whatever the view did.
  dirty: bool,
  /// The target's LOD view changed since the last keying.
  view_moved: bool,
}

/// The total order of an f32 as unsigned bits (negative values below
/// positive, ascending both ways).
fn ordered_bits(f: f32) -> u32 {
  let b = f.to_bits();
  if b & 0x8000_0000 != 0 {
    !b
  } else {
    b | 0x8000_0000
  }
}

/// An entry's packed sort key and its distance to the eye: the queue
/// above all, then render_order, then the depth term - the opaque and
/// cutout distance bucket ascending (front-to-back; the forward depth
/// under an orthographic view, where distance to the eye means nothing),
/// or the transparent forward depth descending (back-to-front). Bind
/// order breaks the ties.
fn draw_key(order: DrawOrder, center: [f32; 3], view: &LodView) -> (u64, f32) {
  let dx = center[0] - view.eye[0];
  let dy = center[1] - view.eye[1];
  let dz = center[2] - view.eye[2];
  let depth = dx * view.forward[0] + dy * view.forward[1] + dz * view.forward[2];
  let distance = if view.ortho { depth.abs() } else { (dx * dx + dy * dy + dz * dz).sqrt() };
  let term = if order.queue == DrawQueue::Transparent {
    !ordered_bits(depth)
  } else {
    let bucket = (distance.max(ORDER_DISTANCE_FLOOR).log2() * ORDER_BUCKETS_PER_DOUBLING).floor() as i32;
    (bucket - ORDER_BUCKET_BASE).max(0) as u32
  };
  let ro = (order.render_order.clamp(-ORDER_RENDER_LIMIT, ORDER_RENDER_LIMIT) + ORDER_RENDER_LIMIT) as u64;
  (((order.queue as u64) << 53) | (ro << 32) | term as u64, distance)
}

/// A fade band's position is quantized to this many steps: the dither
/// hash is continuous, so 64 levels read as a smooth dissolve, and an
/// entry inside a band is rewritten only when its step changes instead
/// of every frame the camera moves.
const LOD_FADE_STEPS: f32 = 64.0;

/// A LOD group's configuration and its per-target choice.
struct LodGroup {
  levels: Vec<LodLevel>,
  /// The cross-fade band as a fraction of each threshold (0 = a hard
  /// switch with hysteresis).
  fade: f32,
  /// The target whose view population levels (record sinks) pick by;
  /// node levels pick per target and ignore it.
  reference: Option<u64>,
  /// One per target that has a view, in the order they were first met.
  states: Vec<LodState>,
}

/// A LOD group's choice on one target: the level drawn (`levels.len()` =
/// culled) and the band position - 1 = solid; below 1 the level keeps
/// that fraction of the pixels and the next level the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LodState {
  target: u64,
  level: u32,
  weight: f32,
}

/// The state a group holds on a target before its first evaluation:
/// unlike any computed one, so the first evaluation always publishes.
const LOD_UNSET: LodState = LodState { target: 0, level: u32::MAX, weight: 1.0 };

/// Pick the level for a measured size `c` (see `LodLevel`), given the
/// level currently held (`u32::MAX` for none yet). Hard switches keep the
/// current level inside its hysteresis band; a fade instead widens each
/// threshold `s` into the band `[s, s * (1 + fade))`, over which the
/// weight runs from 0 (all of the next level) to 1 (all of this one).
fn select_level(levels: &[LodLevel], fade: f32, current: u32, c: f32) -> (u32, f32) {
  let n = levels.len() as u32;
  let plain = |c: f32| levels.iter().position(|l| c >= l.size).map(|p| p as u32).unwrap_or(n);
  if fade > 0.0 {
    let level = plain(c);
    if level < n {
      let s = levels[level as usize].size;
      let top = s * (1.0 + fade);
      if s > 0.0 && c < top {
        let t = ((c - s) / (top - s)).clamp(0.0, 1.0);
        return (level, (t * LOD_FADE_STEPS).round() / LOD_FADE_STEPS);
      }
    }
    return (level, 1.0);
  }
  if current < n {
    let above_floor = c >= levels[current as usize].size;
    let below_ceiling = current == 0 || c < levels[current as usize - 1].size * (1.0 + LOD_HYSTERESIS);
    if above_floor && below_ceiling {
      return (current, 1.0);
    }
  } else if current == n && n > 0 && c < levels[n as usize - 1].size * (1.0 + LOD_HYSTERESIS) {
    return (n, 1.0);
  }
  (plain(c), 1.0)
}

/// The projected size of a sphere: its diameter as a fraction of the
/// viewport height under `view`.
fn projected_size(view: &LodView, center: [f32; 3], radius: f32) -> f32 {
  let d = if view.ortho {
    1.0
  } else {
    let dx = center[0] - view.eye[0];
    let dy = center[1] - view.eye[1];
    let dz = center[2] - view.eye[2];
    (dx * dx + dy * dy + dz * dz).sqrt().max(LOD_MIN_DISTANCE)
  };
  view.bias * radius * view.focal / d
}

/// The level a target draws for a sink without fade support: the band's
/// majority side.
fn dominant_level(state: &LodState) -> u32 {
  // The band midpoint: below it the next level holds most of the pixels.
  const HALF: f32 = 0.5;
  if state.weight >= HALF {
    state.level
  } else {
    state.level + 1
  }
}

/// How an instance-record sink projects the node's transform into its
/// slot's floats. `Pose2D` is `[x, y, angle, sx, sy]`: xy translation,
/// the rotation of the local x axis in the xy plane (`atan2(m[1], m[0])`),
/// and the xy scale, `sy` negated when the matrix mirrors (negative 2x2
/// determinant) so handedness survives the round trip. `Matrix` is the
/// whole matrix, 16 floats column-major: the per-instance model matrix a
/// vertex stage reads as four vec4 columns. Both project the WORLD
/// matrix, or, when the buffer's group carries an anchor (see
/// `set_instance_record`), the world matrix relative to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceProjection {
  Pose2D,
  Matrix,
}

/// A hidden `Matrix` slot: zero scale with the translation's w kept at 1,
/// so the instance's vertices collapse to one point and its zero-area
/// triangles draw nothing. An all-zero matrix would leave w = 0, a
/// homogeneous point the clipper need not reject.
const HIDDEN_MATRIX: [f32; 16] = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
/// A hidden `Pose2D` slot: zero scale collapses the quad.
const HIDDEN_POSE2D: [f32; 5] = [0.0; 5];

impl InstanceProjection {
  /// Floats per record slot.
  pub fn floats(&self) -> u32 {
    match self {
      InstanceProjection::Pose2D => 5,
      InstanceProjection::Matrix => 16,
    }
  }

  /// The slot of a hidden, unbound or destroyed node: draws nothing.
  fn hidden(&self) -> &'static [f32] {
    match self {
      InstanceProjection::Pose2D => &HIDDEN_POSE2D,
      InstanceProjection::Matrix => &HIDDEN_MATRIX,
    }
  }
}

/// Routes a projection of the node's world transform to slot `index` of a
/// vertex buffer used as an instance buffer: floats [index*stride,
/// (index+1)*stride) where stride is the projection's float count. The
/// bridge between the transform hierarchy and instanced rendering - one
/// node per drawn instance, the draw itself untouched. Writes batch: the
/// flush accumulates every slot into a staging mirror and publishes one
/// coalesced dirty range per buffer, so a thousand nodes moved by one
/// producer step cost one buffer write. A hidden node's slot takes the
/// projection's hidden record (zero scale collapses the instance); so
/// does an unbound or destroyed node's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceRecordSink {
  pub buffer: u64,
  pub index: u32,
  pub projection: InstanceProjection,
}

/// Routes the node's world matrix, post-multiplied by a constant matrix,
/// to row `row` of a float texture: 16 floats (one column-major mat4, the
/// four rgba32f texels of that row) at `values[row*16, (row+1)*16)`. The
/// bridge between the transform hierarchy and matrix palettes a vertex
/// shader texelFetches - a skin's bone palette is joint nodes bound row by
/// row with `post` the joint's inverse bind. Writes batch like instance
/// records: the flush stages every changed row and publishes each dirty
/// texture once, whole. A node carries one texture slot per texture.
///
/// The texture's group may carry an ANCHOR node: published rows are then
/// `inverse(anchorWorld) * nodeWorld * post`, making the palette local to
/// the anchor (a model root keeps its skin palette in model space, so the
/// mesh's own `uModel` still places it). The anchor must be an ANCESTOR of
/// every bound node - only then does an anchor move restage every row (its
/// whole subtree recomputes); this is the consumer's contract, unchecked.
/// Rows update while hidden (a palette feeds a mesh whose own sink handles
/// visibility), and an unbound or destroyed node's row keeps its last
/// value (a zeroed bone matrix would collapse the vertices weighted to it;
/// teardown destroys the texture in the same batch anyway).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSlotSink {
  pub texture: u64,
  pub row: u32,
  pub post: Mat4,
}

/// The palette sink's second row kind: the node's WEIGHTS register (a
/// mesh's morph target weights, `set_weights`) published as row `row` of
/// an rgba32f texture `row_floats / 4` texels wide - four weights per
/// texel, the register padded with zeros to the row (or cut to it). A
/// texture holds one kind of row: matrix rows or weights rows, never
/// both. Weights rows stage from the register, not the transform walk,
/// so a weights write costs no matrix recompute; the publish batches like
/// the matrix palettes - one whole-texture write per texture per flush.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeightsSlotSink {
  pub texture: u64,
  pub row: u32,
  pub row_floats: u32,
}

/// One palette texture's staging mirror and the sinks feeding it. For
/// matrix rows `values` holds `nodeWorld * post` per row (16 floats) and
/// the anchor inverse applies at publish; for weights rows it holds the
/// registers, `row_floats` per row, published as they are.
struct PaletteGroup {
  anchor: Option<NodeId>,
  values: Vec<f32>,
  refs: u32,
  dirty: bool,
  row_floats: usize,
  weights: bool,
}

/// One instance buffer's staging mirror and the sinks feeding it: one
/// projection (so one stride) and one anchor per buffer.
struct InstanceGroup {
  projection: InstanceProjection,
  /// The node whose frame the records are relative to; None = world.
  anchor: Option<NodeId>,
  /// `inverse(anchorWorld)` as of flush `inv_flush` (see `anchored`).
  anchor_inv: Mat4,
  inv_flush: u64,
  values: Vec<f32>,
  refs: u32,
  /// Dirty float range [lo, hi) into `values`; None = clean.
  dirty: Option<(usize, usize)>,
}

impl InstanceGroup {
  fn stride(&self) -> usize {
    self.projection.floats() as usize
  }

  fn mark(&mut self, lo: usize, hi: usize) {
    self.dirty = match self.dirty {
      Some((a, b)) => Some((a.min(lo), b.max(hi))),
      None => Some((lo, hi)),
    };
  }

  /// Grow the mirror to hold `slots` records; a new slot starts hidden,
  /// so a gap between bound slots never publishes as raw zeros.
  fn reserve(&mut self, slots: usize) {
    let need = slots * self.stride();
    while self.values.len() < need {
      self.values.extend_from_slice(self.projection.hidden());
    }
  }
}

/// The `Pose2D` decomposition of a world matrix (see `InstanceProjection`).
fn pose2d(m: &Mat4) -> [f32; 5] {
  let sx = (m[0] * m[0] + m[1] * m[1]).sqrt();
  let sy = (m[4] * m[4] + m[5] * m[5]).sqrt();
  let mirrored = m[0] * m[5] - m[1] * m[4] < 0.0;
  [m[12], m[13], m[1].atan2(m[0]), sx, if mirrored { -sy } else { sy }]
}

/// Hand `visit` every world-space triangle of a node that can touch the
/// world box `aabb`: its shape's, through the shape BVH walked with the
/// box in the node's frame, or the twelve of its local box without one.
fn node_triangles(
  shapes: &mut pick::Shapes,
  shape: Option<ShapeId>,
  bounds: &Box3,
  world: &Mat4,
  aabb: &Box3,
  visit: &mut dyn FnMut([[f32; 3]; 3]),
) {
  let carry = |tri: [[f32; 3]; 3]| {
    [transform_point(world, tri[0]), transform_point(world, tri[1]), transform_point(world, tri[2])]
  };
  match shape {
    Some(sid) => {
      let local = world_box(aabb, &invert_affine(world));
      shapes.visit_box(sid, &local, &mut |tri| visit(carry(tri)));
    }
    None => {
      for tri in box_triangles(bounds) {
        visit(carry(tri));
      }
    }
  }
}

/// The twelve triangles of a local box, two per face; every test they
/// feed is two-sided, so the winding is immaterial.
fn box_triangles(bounds: &Box3) -> [[[f32; 3]; 3]; 12] {
  // Corner i has bit 0 = max x, bit 1 = max y, bit 2 = max z.
  let corner = |i: usize| -> [f32; 3] {
    [
      if i & 1 != 0 { bounds[3] } else { bounds[0] },
      if i & 2 != 0 { bounds[4] } else { bounds[1] },
      if i & 4 != 0 { bounds[5] } else { bounds[2] },
    ]
  };
  const FACES: [[usize; 4]; 6] = [[0, 2, 6, 4], [1, 3, 7, 5], [0, 1, 5, 4], [2, 3, 7, 6], [0, 1, 3, 2], [4, 5, 7, 6]];
  let mut out = [[[0.0; 3]; 3]; 12];
  for (f, face) in FACES.iter().enumerate() {
    out[f * 2] = [corner(face[0]), corner(face[1]), corner(face[2])];
    out[f * 2 + 1] = [corner(face[0]), corner(face[2]), corner(face[3])];
  }
  out
}

/// The nearest of a local box's twelve triangles along the local ray:
/// (t, unnormalized local normal), or None.
fn ray_box(bounds: &Box3, o: [f32; 3], d: [f32; 3]) -> Option<(f32, [f32; 3])> {
  let mut best: Option<(f32, [f32; 3])> = None;
  for [a, b, c] in box_triangles(bounds) {
    if let Some((t, _, _, n)) = pick::ray_points(a, b, c, o, d) {
      if best.is_none_or(|(bt, _)| t < bt) {
        best = Some((t, n));
      }
    }
  }
  best
}

/// A world box's bounding sphere (center, half diagonal); without a box
/// the matrix's translation at radius 0.
fn sphere_of(b: Option<Box3>, world: &Mat4) -> ([f32; 3], f32) {
  match b {
    Some(b) => {
      let c = [(b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0];
      let e = [(b[3] - b[0]) / 2.0, (b[4] - b[1]) / 2.0, (b[5] - b[2]) / 2.0];
      (c, (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt())
    }
    None => ([world[12], world[13], world[14]], 0.0),
  }
}

struct Node {
  generation: u32,
  alive: bool,
  parent: Option<u32>,
  children: Vec<u32>,
  position: [f32; 3],
  rotation: [f32; 4],
  scale: [f32; 3],
  local: Mat4,
  world: Mat4,
  /// The local matrix needs recomposing from position/rotation/scale.
  local_dirty: bool,
  /// Queued for the next flush (a transform, visibility or parent change).
  queued: bool,
  /// Recomputed or re-shown by this flush's walk: the cull pass re-tests
  /// every sink (a target's frustum moving re-tests them anyway).
  queued_touch: bool,
  visible: bool,
  /// Effective visibility as of the last flush (every ancestor visible too).
  shown: bool,
  /// On its way out (`exit`): still painted, invisible to every query,
  /// freed when its exit tracks settle and its leaving children are gone.
  leaving: bool,
  /// One per target.
  sinks: Vec<BoundSink>,
  /// Local-space tight box; with one the node has a leaf in the index.
  bounds: Option<Box3>,
  leaf: Option<u32>,
  /// A local box for culling ONLY (a skeleton joint's influence region):
  /// keeps the node out of the picking index while its world box still
  /// follows the flush. Culling reads this, else `bounds`.
  cull_bounds: Option<Box3>,
  /// The world-axis box of `cull_bounds`/`bounds` as of the last
  /// recompute; None without either.
  world_box: Option<Box3>,
  /// Whether frustums gate this node's draw sinks at all (the per-object
  /// opt-out for geometry a vertex stage moves beyond its box).
  cull: bool,
  /// World units the frustum test widens the box by on every side.
  cull_margin: f32,
  /// Nodes whose world boxes, united, stand in for this node's own in the
  /// frustum test (a skinned part culled by its joints' boxes, so the box
  /// follows the pose). Empty = the node's own box.
  cull_group: Vec<NodeId>,
  /// Nodes whose cull group this node is in: a move here re-tests them.
  cull_owners: Vec<NodeId>,
  /// The geometry the node stands for: its local box, and the picking
  /// narrowphase when the shape names triangles. None = the box set by
  /// set_bounds only.
  shape: Option<ShapeId>,
  /// Layer membership bitmask the queries test against their mask
  /// (default 1, Three's Object3D.layers).
  layers: u32,
  /// One per target.
  slots: Vec<SharedSlotSink>,
  /// One per texture.
  texture_slots: Vec<TextureSlotSink>,
  /// The node's weights register (morph target weights): written by
  /// `set_weights`, published through `weights_slots`.
  weights: Vec<f32>,
  /// One per texture.
  weights_slots: Vec<WeightsSlotSink>,
  /// In `weights_dirty` already.
  weights_queued: bool,
  /// One per LOD level (one for a plain population); `record_level`
  /// names the one holding the pose, the others hold the hidden record.
  records: Vec<InstanceRecordSink>,
  record_level: u32,
  /// The record slot holds this node's shown pose (false = zeroed or
  /// never written; the next shown flush writes it).
  record_on: bool,
  /// The LOD group this node heads (`set_lod`).
  lod: Option<Box<LodGroup>>,
  /// This node is a level of its parent's group: (group index, level).
  lod_level: Option<(u32, u32)>,
  /// The nearest level this node lies under, itself included, as of the
  /// last walk - what the gate tests; inherited like `shown`.
  lod_scope: Option<(u32, u32)>,
  /// Moved this flush: the LOD pass re-measures it (a group, or a node
  /// with population levels).
  lod_check: bool,
}

#[derive(Default)]
pub struct Spatial {
  nodes: Vec<Node>,
  free: Vec<u32>,
  queue: Vec<u32>,
  bvh: Bvh,
  pub(crate) shapes: pick::Shapes,
  shared: HashMap<(u64, String), SharedGroup>,
  instances: HashMap<u64, InstanceGroup>,
  palettes: HashMap<u64, PaletteGroup>,
  /// Nodes whose weights register changed since the last flush.
  weights_dirty: Vec<u32>,
  /// The weights tracks' advance buffer, reused across frames.
  weights_scratch: Vec<f32>,
  transitions: NodeTransitions,
  players: players::PlayerSet,
  /// Per target, the clip volume its draw sinks are gated by (a target
  /// without one never culls).
  frustums: HashMap<u64, Frustum>,
  /// Targets whose frustum changed since the last flush: every sink on
  /// them is re-tested by the cull pass.
  frustum_dirty: Vec<u64>,
  /// Nodes the walk recomputed or re-shown this flush: the cull pass
  /// re-tests their sinks even when no frustum moved.
  touched: Vec<u32>,
  /// Counts flushes: what per-flush caches (an instance group's anchor
  /// inverse) are stamped with.
  flush_id: u64,
  /// Per target, what projected size is measured with (a target without
  /// one draws every group's first level).
  lod_views: HashMap<u64, LodView>,
  /// Targets whose view changed since the last flush: every group is
  /// re-measured on them by the LOD pass.
  lod_dirty: Vec<u64>,
  /// The nodes heading a LOD group (dead ones drop at the next pass).
  lod_groups: Vec<u32>,
  /// The nodes carrying more than one record sink: population members
  /// whose level the LOD pass picks.
  lod_records: Vec<u32>,
  /// Groups and population members the walk moved this flush.
  lod_moved: Vec<u32>,
  /// Per target ordering its bound entries (`set_draw_sort`): the sort's
  /// state between flushes.
  draw_sorts: HashMap<u64, DrawSort>,
  /// Bind order counter: every draw sink bound takes the next value.
  bind_seq: u64,
}

/// The layer mask a node starts with: layer 0 alone, Three's default.
const DEFAULT_LAYERS: u32 = 1;

fn index(id: NodeId) -> usize {
  (id & 0xffff_ffff) as usize
}

fn generation(id: NodeId) -> u32 {
  (id >> 32) as u32
}

impl Spatial {
  pub fn new() -> Self {
    Self::default()
  }

  fn resolve(&self, id: NodeId) -> Result<u32, String> {
    let i = index(id);
    match self.nodes.get(i) {
      Some(n) if n.alive && n.generation == generation(id) => Ok(i as u32),
      _ => Err(format!("spatial node {id} not found")),
    }
  }

  fn enqueue(&mut self, i: u32) {
    let n = &mut self.nodes[i as usize];
    if !n.queued {
      n.queued = true;
      self.queue.push(i);
    }
  }

  /// A new root node with the given local transform; `visible` false hides
  /// its whole subtree. Starts queued, so the first flush computes it.
  pub fn create(&mut self, position: [f32; 3], rotation: [f32; 4], scale: [f32; 3], visible: bool) -> NodeId {
    let fresh = Node {
      generation: 0,
      alive: true,
      parent: None,
      children: Vec::new(),
      position,
      rotation,
      scale,
      local: IDENTITY,
      world: IDENTITY,
      local_dirty: true,
      queued: false,
      queued_touch: false,
      visible,
      shown: false,
      leaving: false,
      sinks: Vec::new(),
      bounds: None,
      leaf: None,
      cull_bounds: None,
      world_box: None,
      cull: true,
      cull_margin: 0.0,
      cull_group: Vec::new(),
      cull_owners: Vec::new(),
      shape: None,
      layers: DEFAULT_LAYERS,
      slots: Vec::new(),
      texture_slots: Vec::new(),
      weights: Vec::new(),
      weights_slots: Vec::new(),
      weights_queued: false,
      records: Vec::new(),
      record_level: 0,
      record_on: false,
      lod: None,
      lod_level: None,
      lod_scope: None,
      lod_check: false,
    };
    let i = match self.free.pop() {
      Some(i) => {
        let generation = self.nodes[i as usize].generation.wrapping_add(1);
        self.nodes[i as usize] = Node { generation, ..fresh };
        i
      }
      None => {
        self.nodes.push(fresh);
        (self.nodes.len() - 1) as u32
      }
    };
    self.enqueue(i);
    let id = ((self.nodes[i as usize].generation as u64) << 32) | i as u64;
    // Owed an enter animation at the next advance, should its declaration
    // (set any time before then) carry `from` values.
    self.transitions.entering.push(id);
    id
  }

  /// Free a node NOW, exit or no exit: its children become roots (the
  /// consumer tears a subtree down node by node, so they are usually gone
  /// in the same batch), except leaving ones, which are freed with it - a
  /// corpse never outlives the parent whose frame it was animating in.
  /// Draw sinks are dropped without a write: the entries they pointed at
  /// are the consumer's to remove. On a leaving node this is the cancel
  /// (a re-add, a dispose): the node frees where it stands and lands in
  /// `take_freed` like a settled exit would.
  pub fn destroy(&mut self, id: NodeId) -> Result<(), String> {
    let i = self.resolve(id)?;
    let parent = self.nodes[i as usize].parent;
    self.free_node(i);
    // The parent may have been waiting on this child alone.
    if let Some(p) = parent {
      if self.nodes[p as usize].leaving {
        self.check_exit(p);
      }
    }
    Ok(())
  }

  fn free_node(&mut self, i: u32) {
    let id = self.id_of(i);
    if let Some(p) = self.nodes[i as usize].parent {
      self.nodes[p as usize].children.retain(|&c| c != i);
    }
    let children = std::mem::take(&mut self.nodes[i as usize].children);
    for c in children {
      if self.nodes[c as usize].leaving {
        self.nodes[c as usize].parent = None;
        self.free_node(c);
      } else {
        self.nodes[c as usize].parent = None;
        self.enqueue(c);
      }
    }
    if let Some(leaf) = self.nodes[i as usize].leaf.take() {
      self.bvh.remove(leaf);
    }
    for slot in std::mem::take(&mut self.nodes[i as usize].slots) {
      self.release_slot(&slot);
    }
    for slot in std::mem::take(&mut self.nodes[i as usize].texture_slots) {
      self.release_texture_slot(&slot);
    }
    for slot in std::mem::take(&mut self.nodes[i as usize].weights_slots) {
      self.release_weights_slot(&slot);
    }
    for record in std::mem::take(&mut self.nodes[i as usize].records) {
      self.release_record(&record);
    }
    self.clear_lod(i);
    self.transitions.configs.remove(&id);
    self.transitions.cancel_node(id);
    let n = &mut self.nodes[i as usize];
    if n.leaving {
      self.transitions.freed.push(id);
    }
    n.alive = false;
    n.leaving = false;
    n.parent = None;
    for b in n.sinks.drain(..) {
      if let Some(sort) = self.draw_sorts.get_mut(&b.sink.target) {
        sort.dirty = true;
      }
    }
    n.bounds = None;
    n.cull_bounds = None;
    n.world_box = None;
    n.cull_group.clear();
    n.cull_owners.clear();
    n.shape = None;
    n.lod_level = None;
    n.lod_scope = None;
    n.lod_check = false;
    n.record_level = 0;
    self.free.push(i);
  }

  /// Let go of a node the way its declaration says: every component with
  /// an `exit` animates from where it is now (mid-flight included) to its
  /// exit value, on the exit's own motion, and the node becomes LEAVING -
  /// still painted and flushed, skipped by every query (`collider`),
  /// refused as a parent - until its exit tracks have settled and no
  /// leaving child remains under it, when it frees and lands in
  /// `take_freed`. Children the consumer let go of first are those leaving
  /// children, so a subtree torn down children-first frees root-last, each
  /// corpse in its parent's frame to the end; a child still alive stays
  /// put and becomes a root at the free, as with `destroy`. Returns whether
  /// the node is now leaving: nothing to animate (no exit declared, every
  /// exit value already held, nothing leaving below) frees it at once and
  /// returns false, so the caller can clean up synchronously. No settle
  /// event fires for an exit track, and a second `exit` on a leaving node
  /// changes nothing.
  ///
  /// Under a stagger group (an ancestor declaring `stagger_ms`) the exit
  /// does not start here: the node is leaving at once, and the advance
  /// starts every exit let go of under the group this frame in TREE order
  /// (`start_staggered_exits`), so the cascade reads the same whoever tore
  /// the subtree down and in whatever order - a component unmount (Solid
  /// disposes children last-first), an imperative destroy (first-to-last)
  /// and the element tree's own exit walk all leave first-to-last.
  pub fn exit(&mut self, id: NodeId) -> Result<bool, String> {
    let i = self.resolve(id)?;
    if self.nodes[i as usize].leaving {
      return Ok(true);
    }
    let now = self.transitions.now_ms;
    let has_exit =
      self.transitions.configs.get(&id).is_some_and(|c| COMPONENTS.iter().any(|&component| c.has_exit(component)));
    if has_exit && self.stagger_group_of(i).is_some() {
      self.transitions.staggered_exits.push((id, now));
      self.nodes[i as usize].leaving = true;
      return Ok(true);
    }
    let started = self.start_exit_tracks(i, now, 0.0);
    let waiting = self.nodes[i as usize].children.iter().any(|&c| self.nodes[c as usize].leaving);
    if !started && !waiting {
      self.free_node(i);
      return Ok(false);
    }
    self.nodes[i as usize].leaving = true;
    Ok(true)
  }

  /// One node's exit tracks: per declared exit, a track from the
  /// component's current value (or a held write, with the exit's delay plus
  /// `stagger_ms`) as of `at_ms`. Returns whether anything started or
  /// waits.
  fn start_exit_tracks(&mut self, i: u32, at_ms: f64, stagger_ms: f32) -> bool {
    let id = self.id_of(i);
    let Some(config) = self.transitions.configs.get(&id).cloned() else {
      return false;
    };
    let n = &self.nodes[i as usize];
    let (cur_p, cur_q, cur_s) = (lanes3(n.position), n.rotation, lanes3(n.scale));
    let ends = [
      (Component::Position, cur_p, config.position.and_then(|e| e.exit.map(|x| (lanes3(x.value), x.motion)))),
      (Component::Scale, cur_s, config.scale.and_then(|e| e.exit.map(|x| (lanes3(x.value), x.motion)))),
      (Component::Rotation, cur_q, config.rotation.and_then(|e| e.exit.map(|x| (x.value, x.motion)))),
    ];
    let mut started = false;
    for (component, current, end) in ends {
      let Some((to, motion)) = end else { continue };
      let delay_ms = motion.delay_ms + stagger_ms;
      if delay_ms > 0.0 {
        let at = at_ms + delay_ms as f64;
        self.transitions.schedule(PendingWrite { node: id, component, to, spec: motion.spec, at_ms: at });
        started = true;
      } else {
        self.transitions.unschedule(id, component);
        started |= self.transitions.apply(id, component, current, to, motion.spec, at_ms);
      }
    }
    if let Some(x) = config.weights.and_then(|e| e.exit) {
      let delay_ms = x.motion.delay_ms + stagger_ms;
      if delay_ms > 0.0 {
        let at_ms = at_ms + delay_ms as f64;
        self.transitions.schedule_weights(PendingWeights { node: id, to: x.value, spec: x.motion.spec, at_ms });
        started = true;
      } else {
        self.transitions.unschedule_weights(id);
        let current = self.nodes[i as usize].weights.clone();
        started |= self.transitions.retarget_weights(id, &current, &x.value, x.motion.spec, at_ms);
      }
    }
    started
  }

  /// The exits let go of under stagger groups since the last advance,
  /// started now: grouped by their stagger ancestor, each group's members
  /// in pre-order under it (children order, depth first), indexed in that
  /// order. Each runs as of the clock it was let go of at, so a late
  /// advance changes nothing. A member whose group is gone since plays
  /// unstaggered; one that starts nothing (its exit value already held,
  /// index 0) goes to `exit_checks`, which may free it.
  fn start_staggered_exits(&mut self, exit_checks: &mut Vec<NodeId>) {
    let batch = std::mem::take(&mut self.transitions.staggered_exits);
    if batch.is_empty() {
      return;
    }
    let mut groups: Vec<(u32, Vec<(NodeId, f64)>)> = Vec::new();
    for (id, at_ms) in batch {
      let Ok(i) = self.resolve(id) else {
        continue;
      };
      match self.stagger_group_of(i) {
        Some((g, _)) => match groups.iter_mut().find(|(gi, _)| *gi == g) {
          Some((_, members)) => members.push((id, at_ms)),
          None => groups.push((g, vec![(id, at_ms)])),
        },
        None => {
          if !self.start_exit_tracks(i, at_ms, 0.0) {
            exit_checks.push(id);
          }
        }
      }
    }
    for (g, mut members) in groups {
      let ranks = self.preorder_ranks(g);
      members.sort_by_key(|(id, _)| ranks.get(&(index(*id) as u32)).copied().unwrap_or(usize::MAX));
      let gid = self.id_of(g);
      let stagger_ms = self.transitions.configs.get(&gid).and_then(|c| c.stagger_ms).unwrap_or(0.0);
      for (id, at_ms) in members {
        let Ok(i) = self.resolve(id) else {
          continue;
        };
        let stagger = self.transitions.stagger_index(gid, true) as f32 * stagger_ms;
        if !self.start_exit_tracks(i, at_ms, stagger) {
          exit_checks.push(id);
        }
      }
    }
  }

  /// Every node under `root` (itself excluded) by its pre-order rank:
  /// children order, depth first - the order the element tree's exit walk
  /// numbers a cascade in.
  fn preorder_ranks(&self, root: u32) -> HashMap<u32, usize> {
    let mut ranks = HashMap::new();
    let mut stack: Vec<u32> = self.nodes[root as usize].children.iter().rev().copied().collect();
    while let Some(i) = stack.pop() {
      ranks.insert(i, ranks.len());
      stack.extend(self.nodes[i as usize].children.iter().rev().copied());
    }
    ranks
  }

  /// A leaving node's free gate: free it once no declared exit still
  /// runs or waits on it and no leaving child remains, then look at its
  /// parent, which may have been waiting on exactly this child.
  fn check_exit(&mut self, i: u32) {
    let n = &self.nodes[i as usize];
    if !n.alive || !n.leaving {
      return;
    }
    let id = self.id_of(i);
    // An exit still waiting for its place in a cascade has not run yet.
    if self.transitions.staggered_exits.iter().any(|(node, _)| *node == id) {
      return;
    }
    if let Some(config) = self.transitions.configs.get(&id) {
      for component in COMPONENTS {
        if config.has_exit(component) && self.transitions.any_running(id, component) {
          return;
        }
      }
    }
    if n.children.iter().any(|&c| self.nodes[c as usize].leaving) {
      return;
    }
    let parent = n.parent;
    self.free_node(i);
    if let Some(p) = parent {
      self.check_exit(p);
    }
  }

  /// The leaving nodes freed since the last drain (their exits settled, or
  /// a `destroy` cut them short): the consumer's cue to release what it
  /// kept for them (a record slot, a draw entry).
  pub fn take_freed(&mut self) -> Vec<NodeId> {
    std::mem::take(&mut self.transitions.freed)
  }

  /// Whether the node is on its way out (see `exit`).
  pub fn leaving(&self, id: NodeId) -> Result<bool, String> {
    Ok(self.nodes[self.resolve(id)? as usize].leaving)
  }

  /// The motion in force on the node, per component: running tracks and
  /// held writes (the node dump for probing).
  pub fn motion_of(&self, id: NodeId) -> Result<Vec<MotionState>, String> {
    self.resolve(id)?;
    Ok(self.transitions.motion_of(id))
  }

  fn id_of(&self, i: u32) -> NodeId {
    ((self.nodes[i as usize].generation as u64) << 32) | i as u64
  }

  /// Set (or with None clear) the local tight box of a node without a
  /// shape (a shaped node's box is its shape's: refused). With a box the
  /// node is in the index: its leaf follows the world matrix through the
  /// flush, and hidden nodes stay in (skipped at query time, so unhiding
  /// never queries a stale box).
  pub fn set_bounds(&mut self, id: NodeId, bounds: Option<Box3>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if self.nodes[i as usize].shape.is_some() {
      return Err("the node's box comes from its shape".to_string());
    }
    self.put_bounds(i, bounds);
    Ok(())
  }

  fn put_bounds(&mut self, i: u32, bounds: Option<Box3>) {
    if bounds.is_none() {
      if let Some(leaf) = self.nodes[i as usize].leaf.take() {
        self.bvh.remove(leaf);
      }
    }
    self.nodes[i as usize].bounds = bounds;
    self.enqueue(i);
  }

  /// The clip volume gating every draw sink on `target` (None lifts it):
  /// the target's view-projection, column-major. Entries whose node box
  /// (grown by its margin) falls wholly outside it read instance count 0,
  /// exactly like a hidden node, and come back with a fresh params write.
  /// Nodes without a box, or with culling off, are never gated.
  pub fn set_frustum(&mut self, target: u64, view_proj: Option<Mat4>) {
    let changed = match view_proj {
      Some(m) => {
        let f = Frustum::from_view_proj(&m);
        self.frustums.insert(target, f) != Some(f)
      }
      None => self.frustums.remove(&target).is_some(),
    };
    if changed && !self.frustum_dirty.contains(&target) {
      self.frustum_dirty.push(target);
    }
  }

  /// Whether frustums gate the node's draw sinks, and the world-unit
  /// margin the test widens its box by.
  pub fn set_cull(&mut self, id: NodeId, enabled: bool, margin: f32) -> Result<(), String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    n.cull = enabled;
    n.cull_margin = margin;
    self.touch(i);
    Ok(())
  }

  /// Hand the node to the next cull pass without a recompute (its boxes
  /// are current; only the gate changed).
  fn touch(&mut self, i: u32) {
    let n = &mut self.nodes[i as usize];
    if !n.queued_touch && !n.sinks.is_empty() {
      n.queued_touch = true;
      self.touched.push(i);
    }
  }

  /// A local box for culling only - the node stays out of the picking
  /// index (`set_bounds` is the indexed one). None falls back to `bounds`.
  pub fn set_cull_bounds(&mut self, id: NodeId, bounds: Option<Box3>) -> Result<(), String> {
    let i = self.resolve(id)?;
    self.nodes[i as usize].cull_bounds = bounds;
    self.enqueue(i);
    Ok(())
  }

  /// Cull the node by the union of these nodes' world boxes instead of
  /// its own (empty restores its own). A member without a box, or gone,
  /// contributes nothing; with no member box at all the node is not culled.
  pub fn set_cull_group(&mut self, id: NodeId, members: &[NodeId]) -> Result<(), String> {
    let i = self.resolve(id)?;
    let mut indices = Vec::with_capacity(members.len());
    for &m in members {
      indices.push(self.resolve(m)?);
    }
    let old = std::mem::replace(&mut self.nodes[i as usize].cull_group, members.to_vec());
    for m in old {
      if let Ok(j) = self.resolve(m) {
        self.nodes[j as usize].cull_owners.retain(|&o| o != id);
      }
    }
    for j in indices {
      self.nodes[j as usize].cull_owners.push(id);
    }
    self.touch(i);
    Ok(())
  }

  /// The box the frustum test reads for node `i`: its group's union, else
  /// its own world box. None = nothing to test, never culled.
  fn cull_box(&self, i: u32) -> Option<Box3> {
    let n = &self.nodes[i as usize];
    if n.cull_group.is_empty() {
      return n.world_box;
    }
    let mut acc: Option<Box3> = None;
    for &m in &n.cull_group {
      let Ok(j) = self.resolve(m) else {
        continue;
      };
      if let Some(b) = self.nodes[j as usize].world_box {
        acc = Some(match acc {
          Some(a) => union(&a, &b),
          None => b,
        });
      }
    }
    acc
  }

  /// Whether `target`'s frustum lets node `i` draw.
  fn frustum_allows(&self, i: u32, target: u64) -> bool {
    let n = &self.nodes[i as usize];
    if !n.cull {
      return true;
    }
    let Some(f) = self.frustums.get(&target) else {
      return true;
    };
    match self.cull_box(i) {
      Some(b) => f.intersects(&b, n.cull_margin),
      None => true,
    }
  }

  /// The visibility switches, after the walk: every sink of a touched
  /// node, and every sink on a target whose frustum moved, is set to
  /// "shown and inside the frustum". A flip writes the count; a sink
  /// turning on with a stale entry (bound, or moved while off) gets its
  /// params too. A write that does not land releases the sink.
  fn cull_pass(&mut self, out: &mut dyn SinkWriter) {
    let dirty = std::mem::take(&mut self.frustum_dirty);
    let touched = std::mem::take(&mut self.touched);
    let candidates: Vec<u32> = if dirty.is_empty() {
      touched
    } else {
      (0..self.nodes.len() as u32)
        .filter(|&i| self.nodes[i as usize].alive && !self.nodes[i as usize].sinks.is_empty())
        .collect()
    };
    for i in candidates {
      let n = &self.nodes[i as usize];
      if !n.alive || n.sinks.is_empty() {
        continue;
      }
      let shown = n.shown;
      let world = n.world;
      let every = n.queued_touch;
      let mut sinks = std::mem::take(&mut self.nodes[i as usize].sinks);
      if every {
        for b in &sinks {
          if let Some(sort) = self.draw_sorts.get_mut(&b.sink.target) {
            sort.dirty = true;
          }
        }
      }
      let mut normal: Option<Mat4> = None;
      sinks.retain_mut(|b| {
        let sink = b.sink;
        if !every && !dirty.contains(&sink.target) {
          return true;
        }
        let want = shown && self.frustum_allows(i, sink.target) && self.lod_allows(i, sink.target, sink.fade);
        if want != b.entry_on {
          b.entry_on = want;
          if !out.write_count(sink.target, sink.draw, if want { sink.count } else { 0 }) {
            return false;
          }
          if want && b.fresh {
            if sink.normal && normal.is_none() {
              normal = Some(normal_matrix(&world));
            }
            b.fresh = false;
            if !out.write_params(sink.target, sink.draw, &world, if sink.normal { normal.as_ref() } else { None }) {
              return false;
            }
          }
        }
        // The band position while on; solid again when off, so an entry
        // never keeps a band value it is not drawing with.
        if sink.fade {
          let fade = if want { self.lod_fade(i, sink.target) } else { [1.0, 1.0] };
          if fade != b.fade {
            b.fade = fade;
            return out.write_fade(sink.target, sink.draw, fade);
          }
        }
        true
      });
      let n = &mut self.nodes[i as usize];
      n.sinks = sinks;
      n.queued_touch = false;
    }
  }

  /// The draw order of every sorted target (`set_draw_sort`) whose bound
  /// set or view changed: keys every bound entry from its world box
  /// center against the target's LOD view (see `draw_key`), sorts, and
  /// writes the permutation when it differs from the last written. A
  /// target without a LOD view waits for one. A view move shorter than
  /// the bucket margin of the nearest bucketed center changes no bucketed
  /// key, so with no transparent entry and no other change it is skipped
  /// - unless the view is orthographic and turned, since its bucketed key
  /// is the forward depth.
  fn order_pass(&mut self, out: &mut dyn SinkWriter) {
    let mut sorts = std::mem::take(&mut self.draw_sorts);
    let mut keyed: Vec<(u64, u64, u64)> = Vec::new();
    for (&target, state) in sorts.iter_mut() {
      if !state.dirty && !state.view_moved {
        continue;
      }
      let Some(view) = self.lod_views.get(&target) else {
        continue;
      };
      if !state.dirty && !state.transparent && !(view.ortho && view.forward != state.forward) {
        let dx = view.eye[0] - state.eye[0];
        let dy = view.eye[1] - state.eye[1];
        let dz = view.eye[2] - state.eye[2];
        let limit = ORDER_BUCKET_MARGIN * state.nearest.max(ORDER_DISTANCE_FLOOR);
        if dx * dx + dy * dy + dz * dz < limit * limit {
          state.view_moved = false;
          continue;
        }
      }
      keyed.clear();
      let mut nearest = f32::INFINITY;
      let mut transparent = false;
      for (i, n) in self.nodes.iter().enumerate() {
        if !n.alive || n.sinks.is_empty() {
          continue;
        }
        for b in &n.sinks {
          if b.sink.target != target {
            continue;
          }
          let center = match self.cull_box(i as u32) {
            Some(bx) => [(bx[0] + bx[3]) / 2.0, (bx[1] + bx[4]) / 2.0, (bx[2] + bx[5]) / 2.0],
            None => [n.world[12], n.world[13], n.world[14]],
          };
          let (key, distance) = draw_key(b.sink.order, center, view);
          if b.sink.order.queue == DrawQueue::Transparent {
            transparent = true;
          } else if distance < nearest {
            nearest = distance;
          }
          keyed.push((key, b.seq, b.sink.draw));
        }
      }
      keyed.sort_unstable();
      state.eye = view.eye;
      state.forward = view.forward;
      state.nearest = nearest;
      state.transparent = transparent;
      state.dirty = false;
      state.view_moved = false;
      if keyed.iter().map(|k| k.2).eq(state.last.iter().copied()) {
        continue;
      }
      let order: Vec<u64> = keyed.iter().map(|k| k.2).collect();
      if out.write_order(target, &order) {
        state.last = order;
      }
    }
    self.draw_sorts = sorts;
  }

  /// Whether the core orders `target`'s bound draw entries (see
  /// `DrawOrder`): on, every flush that changed a bound entry or moved the
  /// target's LOD view re-keys them and writes the permutation through
  /// `write_order` when it changed. Off (the default) writes no order: a
  /// shadow tile, an override-material view, a 2d target. Enabling a
  /// target already on re-issues its order at the next flush, changed or
  /// not: the call for an entry the core does not bind joining the
  /// target (a background, which draws first).
  pub fn set_draw_sort(&mut self, target: u64, enabled: bool) {
    if enabled {
      let sort = self.draw_sorts.entry(target).or_default();
      sort.dirty = true;
      sort.last.clear();
    } else {
      self.draw_sorts.remove(&target);
    }
  }

  /// Make the node a LOD group (or with an empty list a plain node
  /// again): `levels` nearest first with strictly descending sizes, each
  /// naming a direct child (node levels) or none at all (population
  /// levels, picked per instance node by its record sinks - see
  /// `set_instance_records`); one form per group, never mixed. `fade` is
  /// the cross-fade band as a fraction of each threshold, 0 for a hard
  /// switch with hysteresis. `reference` is the target population levels
  /// measure by (node levels measure per target). The group measures the
  /// sphere around its own box, else around every level's world boxes.
  pub fn set_lod(&mut self, id: NodeId, levels: &[LodLevel], fade: f32, reference: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if !(fade >= 0.0 && fade.is_finite()) {
      return Err(format!("lod fade must be a finite fraction >= 0, got {fade}"));
    }
    let mut members = Vec::with_capacity(levels.len());
    for (k, level) in levels.iter().enumerate() {
      if !(level.size >= 0.0 && level.size.is_finite()) {
        return Err(format!("lod level {k} size must be finite and >= 0, got {}", level.size));
      }
      if k > 0 && level.size >= levels[k - 1].size {
        return Err(format!(
          "lod level sizes must be strictly descending (level {k}: {} after {})",
          level.size,
          levels[k - 1].size
        ));
      }
      if level.node.is_some() != levels[0].node.is_some() {
        return Err("lod levels are all nodes or all population levels, not a mix".to_string());
      }
      if let Some(node) = level.node {
        let j = self.resolve(node)?;
        if self.nodes[j as usize].parent != Some(i) {
          return Err(format!("lod level {k} (node {node}) is not a direct child of node {id}"));
        }
        if members.contains(&j) {
          return Err(format!("lod level {k} names node {node} twice"));
        }
        members.push(j);
      }
    }
    self.clear_lod(i);
    if levels.is_empty() {
      self.enqueue(i);
      return Ok(());
    }
    for (k, &j) in members.iter().enumerate() {
      self.nodes[j as usize].lod_level = Some((i, k as u32));
    }
    self.nodes[i as usize].lod =
      Some(Box::new(LodGroup { levels: levels.to_vec(), fade, reference, states: Vec::new() }));
    if !self.lod_groups.contains(&i) {
      self.lod_groups.push(i);
    }
    // The subtree recomputes (scopes propagate) and the pass re-measures.
    self.enqueue(i);
    Ok(())
  }

  /// Drop the node's group: its levels' membership goes with it, and every
  /// entry under them re-tests (all draw again).
  fn clear_lod(&mut self, i: u32) {
    let Some(group) = self.nodes[i as usize].lod.take() else {
      return;
    };
    for level in &group.levels {
      if let Some(node) = level.node {
        if let Ok(j) = self.resolve(node) {
          self.nodes[j as usize].lod_level = None;
          self.touch_subtree(j);
        }
      }
    }
    self.lod_groups.retain(|&g| g != i);
  }

  /// What `target` measures projected size with (None lifts it: the
  /// target draws every group's first level again). Read at flush, like
  /// the frustum.
  pub fn set_lod_view(&mut self, target: u64, view: Option<LodView>) {
    let changed = match view {
      Some(v) => self.lod_views.insert(target, v) != Some(v),
      None => self.lod_views.remove(&target).is_some(),
    };
    if changed && !self.lod_dirty.contains(&target) {
      self.lod_dirty.push(target);
    }
    if changed {
      if let Some(sort) = self.draw_sorts.get_mut(&target) {
        sort.view_moved = true;
      }
    }
  }

  /// Hand every sink-carrying node under `i` (itself included) to the
  /// next cull pass.
  fn touch_subtree(&mut self, i: u32) {
    self.touch(i);
    let mut k = 0;
    while k < self.nodes[i as usize].children.len() {
      let c = self.nodes[i as usize].children[k];
      self.touch_subtree(c);
      k += 1;
    }
  }

  /// The box enclosing every world box under `i`, itself included.
  fn subtree_box(&self, i: u32, acc: &mut Option<Box3>) {
    let n = &self.nodes[i as usize];
    if let Some(b) = n.world_box {
      *acc = Some(match acc {
        Some(a) => union(a, &b),
        None => b,
      });
    }
    for &c in &n.children {
      self.subtree_box(c, acc);
    }
  }

  /// The sphere a LOD group is measured by: its own world box, else the
  /// union of its levels' subtrees' boxes, else its position at radius 0.
  fn lod_sphere(&self, g: u32) -> ([f32; 3], f32) {
    let n = &self.nodes[g as usize];
    let mut b = n.world_box;
    if b.is_none() {
      if let Some(group) = &n.lod {
        for level in &group.levels {
          if let Some(node) = level.node {
            if let Ok(j) = self.resolve(node) {
              self.subtree_box(j, &mut b);
            }
          }
        }
      }
    }
    sphere_of(b, &n.world)
  }

  /// Re-measure every group and population member the flush moved, and
  /// all of them on the targets whose view changed: a group whose choice
  /// changed on a target hands the levels involved to the cull pass, a
  /// member whose level changed re-stages its record.
  fn lod_pass(&mut self) {
    let dirty = std::mem::take(&mut self.lod_dirty);
    let moved = std::mem::take(&mut self.lod_moved);
    let mut groups = std::mem::take(&mut self.lod_groups);
    groups.retain(|&g| self.nodes[g as usize].alive && self.nodes[g as usize].lod.is_some());
    let views: Vec<(u64, LodView)> = self.lod_views.iter().map(|(&t, &v)| (t, v)).collect();
    for &g in &groups {
      let check = std::mem::replace(&mut self.nodes[g as usize].lod_check, false);
      let (center, radius) = self.lod_sphere(g);
      for &(target, view) in &views {
        let group = self.nodes[g as usize].lod.as_ref().expect("retained above");
        let known = group.states.iter().position(|st| st.target == target);
        if !(check || known.is_none() || dirty.contains(&target)) {
          continue;
        }
        let old = known.map(|k| group.states[k]).unwrap_or(LodState { target, ..LOD_UNSET });
        let c = projected_size(&view, center, radius);
        let (level, weight) = select_level(&group.levels, group.fade, old.level, c);
        let new = LodState { target, level, weight };
        if new == old {
          continue;
        }
        let group = self.nodes[g as usize].lod.as_mut().expect("retained above");
        match known {
          Some(k) => group.states[k] = new,
          None => group.states.push(new),
        }
        // Every level that was or is drawn on this target re-tests, the
        // outgoing ones first (off before on).
        let mut involved = Vec::with_capacity(4);
        if old.level != u32::MAX {
          involved.push(old.level);
          involved.push(old.level + 1);
        }
        involved.push(new.level);
        involved.push(new.level + 1);
        for level in involved {
          let node =
            self.nodes[g as usize].lod.as_ref().and_then(|group| group.levels.get(level as usize)).and_then(|l| l.node);
          if let Some(node) = node {
            if let Ok(j) = self.resolve(node) {
              self.touch_subtree(j);
            }
          }
        }
      }
    }
    // Lifted views: a target no longer measured draws the first level.
    for &t in &dirty {
      if self.lod_views.contains_key(&t) {
        continue;
      }
      for &g in &groups {
        let group = self.nodes[g as usize].lod.as_mut().expect("retained above");
        if let Some(k) = group.states.iter().position(|st| st.target == t) {
          group.states.remove(k);
          self.touch_subtree(g);
        }
      }
    }
    self.lod_groups = groups;
    let mut records = std::mem::take(&mut self.lod_records);
    records.retain(|&r| self.nodes[r as usize].alive && self.nodes[r as usize].records.len() > 1);
    for &r in &records {
      let check = std::mem::replace(&mut self.nodes[r as usize].lod_check, false);
      let Some(anchor) = self.instances.get(&self.nodes[r as usize].records[0].buffer).and_then(|g| g.anchor) else {
        continue;
      };
      let Ok(a) = self.resolve(anchor) else {
        continue;
      };
      let Some((reference, level_count)) =
        self.nodes[a as usize].lod.as_ref().map(|g| (g.reference, g.levels.len() as u32))
      else {
        continue;
      };
      let Some(target) = reference else {
        continue;
      };
      if !(check || moved.contains(&r) || dirty.contains(&target)) {
        continue;
      }
      let Some(view) = self.lod_views.get(&target).copied() else {
        continue;
      };
      let n = &self.nodes[r as usize];
      let (center, radius) = sphere_of(n.world_box, &n.world);
      let current = n.record_level;
      let count = n.records.len() as u32;
      let c = projected_size(&view, center, radius);
      let levels = &self.nodes[a as usize].lod.as_ref().expect("checked above").levels;
      let (mut level, _) = select_level(levels, 0.0, current, c);
      // Fewer sinks than levels: the last sink stands for the rest; a
      // culled level hides the record in every sink.
      let hidden = level >= level_count;
      if level >= count && !hidden {
        level = count - 1;
      }
      if level == current {
        continue;
      }
      let world = self.nodes[r as usize].world;
      let shown = self.nodes[r as usize].shown;
      let record_on = self.nodes[r as usize].record_on;
      if let Some(old) = self.nodes[r as usize].records.get(current as usize).copied() {
        if record_on {
          self.stage_record(&old, old.projection.hidden());
        }
      }
      self.nodes[r as usize].record_level = level;
      if hidden {
        self.nodes[r as usize].record_on = false;
        continue;
      }
      let new = self.nodes[r as usize].records[level as usize];
      if shown {
        let m = self.anchored(new.buffer, &world);
        match new.projection {
          InstanceProjection::Pose2D => self.stage_record(&new, &pose2d(&m)),
          InstanceProjection::Matrix => self.stage_record(&new, &m),
        }
        self.nodes[r as usize].record_on = true;
      }
    }
    self.lod_records = records;
  }

  /// Whether the LOD groups above node `i` let it draw on `target`: every
  /// level it lies under (nested groups chain) must be the one that
  /// target picked - or, for a sink that fades, either level of an open
  /// band. A target without a view draws the first level.
  fn lod_allows(&self, i: u32, target: u64, fade: bool) -> bool {
    let mut scope = self.nodes[i as usize].lod_scope;
    while let Some((g, level)) = scope {
      let gn = &self.nodes[g as usize];
      let Some(group) = &gn.lod else {
        break;
      };
      let state = group.states.iter().find(|st| st.target == target).copied().unwrap_or(LodState {
        target,
        level: 0,
        weight: 1.0,
      });
      let drawn = if fade && state.weight < 1.0 {
        level == state.level || level == state.level + 1
      } else {
        level == dominant_level(&state)
      };
      if !drawn {
        return false;
      }
      scope = gn.lod_scope;
    }
    true
  }

  /// The `uLodFade` value node `i` draws with on `target`: the innermost
  /// open band's position and which side of it this level keeps.
  fn lod_fade(&self, i: u32, target: u64) -> [f32; 2] {
    let Some((g, level)) = self.nodes[i as usize].lod_scope else {
      return [1.0, 1.0];
    };
    let Some(group) = &self.nodes[g as usize].lod else {
      return [1.0, 1.0];
    };
    let Some(state) = group.states.iter().find(|st| st.target == target) else {
      return [1.0, 1.0];
    };
    if state.weight >= 1.0 {
      [1.0, 1.0]
    } else if level == state.level {
      [state.weight, 1.0]
    } else if level == state.level + 1 {
      [state.weight, -1.0]
    } else {
      [1.0, 1.0]
    }
  }

  /// Give the node a shape (or with None take it away): its local box is
  /// the shape's from here on, following every update of the shape, and
  /// its triangles are the narrowphase when the shape names any. A shape
  /// with no vertices leaves the node without a box.
  pub fn set_shape(&mut self, id: NodeId, shape: Option<ShapeId>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let bounds = match shape {
      Some(sid) => self.shapes.bounds(sid).ok_or_else(|| format!("spatial shape {sid} not found"))?,
      None => None,
    };
    self.nodes[i as usize].shape = shape;
    self.put_bounds(i, bounds);
    Ok(())
  }

  /// Bind the node's shared-slot sink on the sink's (target, param name),
  /// replacing the one it had there (the abandoned slot zeroes); a sink
  /// on another param of the same target stays. Binding seeds the slot
  /// at the next flush. The caller flushes afterwards (the JS scheduler
  /// always does).
  /// The node's layer mask, what a query's `layers` is tested against.
  /// Query-only: no flush needed.
  pub fn set_layers(&mut self, id: NodeId, layers: u32) -> Result<(), String> {
    let i = self.resolve(id)?;
    self.nodes[i as usize].layers = layers;
    Ok(())
  }

  pub fn bind_shared_slot(&mut self, id: NodeId, sink: SharedSlotSink) -> Result<(), String> {
    let i = self.resolve(id)?;
    if sink.len == 0 || sink.len % 3 != 0 {
      return Err(format!("shared-slot sink len {} is not a multiple of 3", sink.len));
    }
    if (sink.index + 1) * 3 > sink.len {
      return Err(format!("shared-slot sink slot {} does not fit {} floats", sink.index, sink.len));
    }
    let group = self.shared.entry((sink.target, sink.name.clone())).or_insert_with(|| SharedGroup {
      values: vec![0.0; sink.len as usize],
      refs: 0,
      dirty: false,
    });
    if group.values.len() != sink.len as usize {
      return Err(format!(
        "shared param '{}' on target {} is {} floats, not {}",
        sink.name,
        sink.target,
        group.values.len(),
        sink.len
      ));
    }
    group.refs += 1;
    let slots = &mut self.nodes[i as usize].slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let replaced = s.target == sink.target && s.name == sink.name;
      if replaced {
        released.push(s.clone());
      }
      !replaced
    });
    for slot in &released {
      self.release_slot(slot);
    }
    self.nodes[i as usize].slots.push(sink);
    self.enqueue(i);
    Ok(())
  }

  /// Remove the node's slot sink on `target` (or every slot sink with
  /// None); the abandoned slots zero at the next flush.
  pub fn unbind_shared_slot(&mut self, id: NodeId, target: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let slots = &mut self.nodes[i as usize].slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let keep = target.is_some_and(|t| t != s.target);
      if !keep {
        released.push(s.clone());
      }
      keep
    });
    for slot in &released {
      self.release_slot(slot);
    }
    self.enqueue(i);
    Ok(())
  }

  /// Bind the node's texture slot on the sink's texture, replacing the one
  /// it had there (a re-bind to another row abandons the old row, which
  /// keeps its last value). Every bind on one texture must name the same
  /// `anchor` (one anchor per palette, like one stride per instance
  /// buffer); the first bind sets it. Fit against the actual texture is
  /// the caller's check (Context validates at bind); the staging mirror
  /// grows to the highest bound row. Binding stages the row at the next
  /// flush - the queued node recomputes unconditionally.
  pub fn bind_texture_slot(&mut self, id: NodeId, sink: TextureSlotSink, anchor: Option<NodeId>) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(a) = anchor {
      self.resolve(a)?;
    }
    let group = self.palettes.entry(sink.texture).or_insert_with(|| PaletteGroup {
      anchor,
      values: Vec::new(),
      refs: 0,
      dirty: false,
      row_floats: 16,
      weights: false,
    });
    if group.refs > 0 && group.weights {
      return Err(format!("texture {} holds weights rows, not matrix rows (one row kind per texture)", sink.texture));
    }
    if group.refs > 0 && group.anchor != anchor {
      return Err(format!(
        "texture {} palette is anchored to {:?}, not {:?} (one anchor per texture)",
        sink.texture, group.anchor, anchor
      ));
    }
    group.anchor = anchor;
    if group.refs == 0 {
      // An unreferenced group (released, not yet dropped by the flush)
      // takes the kind of its next bind.
      group.weights = false;
      group.row_floats = 16;
    }
    let need = (sink.row as usize + 1) * 16;
    if group.values.len() < need {
      group.values.resize(need, 0.0);
    }
    group.refs += 1;
    let slots = &mut self.nodes[i as usize].texture_slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let replaced = s.texture == sink.texture;
      if replaced {
        released.push(*s);
      }
      !replaced
    });
    for slot in &released {
      self.release_texture_slot(slot);
    }
    self.nodes[i as usize].texture_slots.push(sink);
    self.enqueue(i);
    Ok(())
  }

  /// Remove the node's texture slot on `texture` (or every texture slot
  /// with None); the abandoned rows keep their last value.
  pub fn unbind_texture_slot(&mut self, id: NodeId, texture: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let slots = &mut self.nodes[i as usize].texture_slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let keep = texture.is_some_and(|t| t != s.texture);
      if !keep {
        released.push(*s);
      }
      keep
    });
    for slot in &released {
      self.release_texture_slot(slot);
    }
    Ok(())
  }

  /// Drop one texture slot's claim on its group; the group itself is
  /// dropped at the next flush once unreferenced. The row is NOT zeroed
  /// (see `TextureSlotSink`).
  fn release_texture_slot(&mut self, sink: &TextureSlotSink) {
    if let Some(group) = self.palettes.get_mut(&sink.texture) {
      group.refs = group.refs.saturating_sub(1);
    }
  }

  /// Write the node's weights register (see `WeightsSlotSink`): staged to
  /// its weights rows at the next flush, and what `weights_of` reads back.
  /// An equal write is free.
  pub fn set_weights(&mut self, id: NodeId, weights: &[f32]) -> Result<(), String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    if n.weights.as_slice() == weights {
      return Ok(());
    }
    n.weights.clear();
    n.weights.extend_from_slice(weights);
    self.mark_weights(i);
    Ok(())
  }

  /// Write the weights register THROUGH a motion: `motion` given, the
  /// write animates on it whatever the node declares (a one-off spec on
  /// the write); otherwise the declaration's weights entry (or `all`)
  /// decides, and without one the write snaps, exactly `set_weights`.
  /// A write matching the running track's target, the held write's
  /// target or the register at rest is left alone. Returns whether
  /// anything changed - the caller's frame-demand signal. A raw
  /// `set_weights` never consults or cancels the track: it overwrites at
  /// the next advance (last write wins).
  pub fn write_weights(&mut self, id: NodeId, weights: &[f32], motion: Option<NodeMotion>) -> Result<bool, String> {
    let i = self.resolve(id)?;
    let motion = match motion {
      Some(m) => Some(m),
      None => self.transitions.configs.get(&id).and_then(|c| c.motion_for(Component::Weights)),
    };
    let Some(motion) = motion else {
      if self.nodes[i as usize].weights.as_slice() == weights {
        return Ok(false);
      }
      self.set_weights(id, weights)?;
      return Ok(true);
    };
    let current = self.nodes[i as usize].weights.clone();
    let unchanged = current.as_slice() == weights && !self.transitions.any_running(id, Component::Weights);
    Ok(self.transitions.write_weights(id, &current, weights, motion, unchanged))
  }

  /// The node's weights register as the arena holds it.
  pub fn weights_of(&self, id: NodeId) -> Result<&[f32], String> {
    let i = self.resolve(id)?;
    Ok(&self.nodes[i as usize].weights)
  }

  fn mark_weights(&mut self, i: u32) {
    let n = &mut self.nodes[i as usize];
    if !n.weights_queued {
      n.weights_queued = true;
      self.weights_dirty.push(i);
    }
  }

  /// Bind the node's weights register to row `sink.row` of `sink.texture`
  /// (see `WeightsSlotSink`). Every bind on one texture must carry the
  /// same `row_floats` (one width per texture); fit against the actual
  /// texture is the caller's check (Context validates at bind). The row
  /// is staged at the next flush. Rebinding the same texture replaces the
  /// node's slot there; another texture adds one.
  pub fn bind_weights_slot(&mut self, id: NodeId, sink: WeightsSlotSink) -> Result<(), String> {
    let i = self.resolve(id)?;
    if sink.row_floats == 0 || sink.row_floats % 4 != 0 {
      return Err(format!("weights rows are whole rgba32f texels, not {} floats", sink.row_floats));
    }
    let group = self.palettes.entry(sink.texture).or_insert_with(|| PaletteGroup {
      anchor: None,
      values: Vec::new(),
      refs: 0,
      dirty: false,
      row_floats: sink.row_floats as usize,
      weights: true,
    });
    if group.refs > 0 && !group.weights {
      return Err(format!("texture {} holds matrix rows, not weights rows (one row kind per texture)", sink.texture));
    }
    if group.refs > 0 && group.row_floats != sink.row_floats as usize {
      return Err(format!(
        "texture {} weights rows are {} floats wide, not {} (one width per texture)",
        sink.texture, group.row_floats, sink.row_floats
      ));
    }
    if group.refs == 0 {
      group.weights = true;
      group.row_floats = sink.row_floats as usize;
      group.anchor = None;
    }
    let need = (sink.row as usize + 1) * group.row_floats;
    if group.values.len() < need {
      group.values.resize(need, 0.0);
    }
    group.refs += 1;
    let slots = &mut self.nodes[i as usize].weights_slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let replaced = s.texture == sink.texture;
      if replaced {
        released.push(*s);
      }
      !replaced
    });
    for slot in &released {
      self.release_weights_slot(slot);
    }
    self.nodes[i as usize].weights_slots.push(sink);
    self.mark_weights(i);
    Ok(())
  }

  /// Remove the node's weights slot on `texture` (or every weights slot
  /// with None); the abandoned rows keep their last value.
  pub fn unbind_weights_slot(&mut self, id: NodeId, texture: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let slots = &mut self.nodes[i as usize].weights_slots;
    let mut released = Vec::new();
    slots.retain(|s| {
      let keep = texture.is_some_and(|t| t != s.texture);
      if !keep {
        released.push(*s);
      }
      keep
    });
    for slot in &released {
      self.release_weights_slot(slot);
    }
    Ok(())
  }

  fn release_weights_slot(&mut self, sink: &WeightsSlotSink) {
    if let Some(group) = self.palettes.get_mut(&sink.texture) {
      group.refs = group.refs.saturating_sub(1);
    }
  }

  /// Stage every changed weights register into its rows: the register
  /// padded with zeros to the row width, or cut to it.
  fn stage_weights(&mut self) {
    if self.weights_dirty.is_empty() {
      return;
    }
    let dirty = std::mem::take(&mut self.weights_dirty);
    for i in dirty {
      let n = &mut self.nodes[i as usize];
      n.weights_queued = false;
      if !n.alive {
        continue;
      }
      for slot in &n.weights_slots {
        let Some(group) = self.palettes.get_mut(&slot.texture) else { continue };
        let width = group.row_floats;
        let at = slot.row as usize * width;
        for k in 0..width {
          let v = n.weights.get(k).copied().unwrap_or(0.0);
          if group.values[at + k] != v {
            group.values[at + k] = v;
            group.dirty = true;
          }
        }
      }
    }
  }

  /// Bind (or with None unbind) the node's instance-record sink. Binding
  /// writes the slot at the next flush; unbinding hides it there. Every
  /// sink on one buffer must carry the same projection (one stride per
  /// buffer) and name the same `anchor`: the node whose frame the records
  /// are relative to (`inverse(anchorWorld) * world` - a mesh's instances
  /// stay in the mesh's own space, so its uModel still places the whole
  /// population), None for plain world records. The anchor must be an
  /// ANCESTOR of every bound node, so an anchor move restages every slot
  /// (its subtree recomputes): the consumer's contract, unchecked, as for
  /// the palette anchor. Slot fit against the actual GPU buffer is the
  /// caller's check (Context validates at bind); the staging mirror grows
  /// to the highest bound slot.
  pub fn set_instance_record(
    &mut self,
    id: NodeId,
    sink: Option<InstanceRecordSink>,
    anchor: Option<NodeId>,
  ) -> Result<(), String> {
    self.set_instance_records(id, sink.into_iter().collect(), anchor)
  }

  /// The population-level form of `set_instance_record`: one sink per LOD
  /// level of the anchor's group (`set_lod` with population levels), on
  /// distinct buffers sharing one projection. The LOD pass measures the
  /// node's own projected size on the group's reference target and stages
  /// its pose into the level picked, the hidden record into the others.
  /// A node with one sink and no group simply draws that one; an empty
  /// list unbinds.
  pub fn set_instance_records(
    &mut self,
    id: NodeId,
    sinks: Vec<InstanceRecordSink>,
    anchor: Option<NodeId>,
  ) -> Result<(), String> {
    let i = self.resolve(id)?;
    if let Some(a) = anchor {
      if !sinks.is_empty() {
        self.resolve(a)?;
      }
    }
    for (k, sink) in sinks.iter().enumerate() {
      if sinks[..k].iter().any(|other| other.buffer == sink.buffer) {
        return Err(format!("instance buffer {} is bound twice on one node", sink.buffer));
      }
      if sink.projection != sinks[0].projection {
        return Err("instance record levels must share one projection".to_string());
      }
    }
    for sink in &sinks {
      let group = self.instances.entry(sink.buffer).or_insert_with(|| InstanceGroup {
        projection: sink.projection,
        anchor,
        anchor_inv: IDENTITY,
        inv_flush: 0,
        values: Vec::new(),
        refs: 0,
        dirty: None,
      });
      if group.projection != sink.projection {
        return Err(format!(
          "instance buffer {} carries {:?} records, not {:?} (one projection per buffer)",
          sink.buffer, group.projection, sink.projection
        ));
      }
      if group.refs > 0 && group.anchor != anchor {
        return Err(format!(
          "instance buffer {} records are anchored to {:?}, not {:?} (one anchor per buffer)",
          sink.buffer, group.anchor, anchor
        ));
      }
      group.anchor = anchor;
      group.reserve(sink.index as usize + 1);
      group.refs += 1;
    }
    for old in std::mem::take(&mut self.nodes[i as usize].records) {
      self.release_record(&old);
    }
    let leveled = sinks.len() > 1;
    let n = &mut self.nodes[i as usize];
    n.records = sinks;
    n.record_level = 0;
    n.record_on = false;
    if leveled && !self.lod_records.contains(&i) {
      self.lod_records.push(i);
    }
    self.enqueue(i);
    Ok(())
  }

  /// The floats the record sinks on `buffer` occupy (through the highest
  /// bound slot); None when no sink names the buffer. What the caller
  /// checks a retarget destination's size against.
  pub fn records_extent(&self, buffer: u64) -> Option<usize> {
    self.instances.get(&buffer).map(|group| group.values.len())
  }

  /// Move every record sink on buffer `old` to buffer `new`: the staging
  /// mirror moves with them and the whole used range republishes at the
  /// next flush, so a population outgrowing its buffer swaps in a larger
  /// one with ONE call and ONE bulk write instead of a rebind per node.
  /// Slot indices are untouched. The caller validates that `new` exists
  /// and fits (`records_extent`); `new` must not already carry records.
  pub fn retarget_records(&mut self, old: u64, new: u64) -> Result<(), String> {
    if old == new {
      return Ok(());
    }
    if !self.instances.contains_key(&old) {
      return Err(format!("no instance records are bound to buffer {old}"));
    }
    if self.instances.contains_key(&new) {
      return Err(format!("buffer {new} already carries instance records"));
    }
    let mut group = self.instances.remove(&old).expect("source group checked above");
    if !group.values.is_empty() {
      group.dirty = Some((0, group.values.len()));
    }
    self.instances.insert(new, group);
    for n in self.nodes.iter_mut() {
      for record in n.records.iter_mut() {
        if record.buffer == old {
          record.buffer = new;
        }
      }
    }
    Ok(())
  }

  /// Drop one record sink's claim on its group: the slot hides at the
  /// next flush, and the group itself is dropped there once unreferenced.
  fn release_record(&mut self, sink: &InstanceRecordSink) {
    if let Some(group) = self.instances.get_mut(&sink.buffer) {
      let stride = group.stride();
      let at = sink.index as usize * stride;
      let hidden = group.projection.hidden();
      if group.values[at..at + stride] != *hidden {
        group.values[at..at + stride].copy_from_slice(hidden);
        group.mark(at, at + stride);
      }
      group.refs = group.refs.saturating_sub(1);
    }
  }

  /// Stage one record slot's fresh values; a no-op when they are unchanged.
  fn stage_record(&mut self, sink: &InstanceRecordSink, values: &[f32]) {
    if let Some(group) = self.instances.get_mut(&sink.buffer) {
      let at = sink.index as usize * group.stride();
      let slot = &mut group.values[at..at + values.len()];
      if slot != values {
        slot.copy_from_slice(values);
        group.mark(at, at + values.len());
      }
    }
  }

  /// `world` in the buffer's anchor frame (`inverse(anchorWorld) * world`;
  /// `world` itself without an anchor). The inverse is taken once per
  /// flush per buffer: the walk visits an anchor before its descendants,
  /// so it is fresh by the time a bound node stages. A dead anchor reads
  /// as identity (a consumer tears instances down before their mesh).
  fn anchored(&mut self, buffer: u64, world: &Mat4) -> Mat4 {
    let (anchor, stale) = match self.instances.get(&buffer) {
      Some(group) => (group.anchor, group.inv_flush != self.flush_id),
      None => return *world,
    };
    let Some(anchor) = anchor else { return *world };
    if stale {
      let inv = match self.resolve(anchor) {
        Ok(a) => invert_affine(&self.nodes[a as usize].world),
        Err(_) => IDENTITY,
      };
      let flush_id = self.flush_id;
      let group = self.instances.get_mut(&buffer).expect("group found above");
      group.anchor_inv = inv;
      group.inv_flush = flush_id;
    }
    multiply(self.instances[&buffer].anchor_inv, *world)
  }

  /// Drop one sink's claim on its group: the slot zeroes at the next
  /// flush, and the group itself is dropped there once unreferenced.
  fn release_slot(&mut self, sink: &SharedSlotSink) {
    if let Some(group) = self.shared.get_mut(&(sink.target, sink.name.clone())) {
      let at = sink.index as usize * 3;
      group.values[at..at + 3].fill(0.0);
      group.dirty = true;
      group.refs = group.refs.saturating_sub(1);
    }
  }

  pub fn create_shape(&mut self, shape: Shape) -> Result<ShapeId, String> {
    self.shapes.create(shape)
  }

  pub fn destroy_shape(&mut self, id: ShapeId) -> Result<(), String> {
    self.shapes.destroy(id)
  }

  /// Rewrite a vertex range of a shape in place (see `Shapes::update`);
  /// the nodes carrying it refit their boxes at the next flush.
  pub fn update_shape(
    &mut self,
    id: ShapeId,
    first: usize,
    positions: &[f32],
    uvs: Option<&[f32]>,
  ) -> Result<(), String> {
    self.shapes.update(id, first, positions, uvs)
  }

  /// Carry the boxes of shapes updated since the last flush onto the
  /// nodes holding them, queued so the walk refits their leaves.
  fn refit_shaped(&mut self) {
    if !self.shapes.any_dirty() {
      return;
    }
    for i in 0..self.nodes.len() as u32 {
      let n = &self.nodes[i as usize];
      if !n.alive {
        continue;
      }
      let Some(bounds) = n.shape.and_then(|sid| self.shapes.dirty_bounds(sid)) else {
        continue;
      };
      self.put_bounds(i, bounds);
    }
    self.shapes.clear_dirty();
  }

  /// The leaf's tight world box from the local box carried through the
  /// world matrix (the standard AABB-of-a-transformed-AABB construction).
  fn refit_leaf(&mut self, i: u32) {
    let n = &self.nodes[i as usize];
    let Some(b) = n.bounds else {
      return;
    };
    let tight = world_box(&b, &n.world);
    match n.leaf {
      Some(leaf) => self.bvh.update(leaf, &tight),
      None => {
        let leaf = self.bvh.insert(i, &tight);
        self.nodes[i as usize].leaf = Some(leaf);
      }
    }
  }

  /// Longest root-to-leaf path of the index (tests only).
  #[cfg(test)]
  pub(crate) fn index_depth(&self) -> usize {
    self.bvh.depth()
  }

  /// Every shown node with bounds the ray strikes, nearest first. A node
  /// with a shape is tested per triangle (hit carries `face`, `uv` when the
  /// shape has UVs, and the world-space geometric `normal`); without one
  /// its local box's twelve triangles are (a `normal` from the face, no
  /// `face`/`uv`), so a ray from inside meets the far side - the surface
  /// contract of `overlap`/`sweep`. The first ray reaching a large shape
  /// builds its triangle BVH (see pick.rs), so repeated rays against a
  /// merged scene stay log-cost. `direction` need not be normalized;
  /// distances are world units.
  pub fn raycast(&mut self, origin: [f32; 3], direction: [f32; 3], filter: &QueryFilter) -> Result<Vec<Hit>, String> {
    let len = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
    if len == 0.0 {
      return Ok(Vec::new());
    }
    let root = self.filter_root(filter)?;
    let d = [direction[0] / len, direction[1] / len, direction[2] / len];
    let mut candidates = Vec::new();
    self.bvh.raycast(origin, d, &mut |i| candidates.push(i));
    let mut hits = Vec::new();
    for i in candidates {
      let Some((bounds, world, shape)) = self.collider(i, filter, root) else {
        continue;
      };
      // The ray in the node's local frame: an affine map preserves the
      // ray parameter, so with the local direction left unnormalized t
      // stays in world units.
      let inv = invert_affine(&world);
      let lo = transform_point(&inv, origin);
      let ld = transform_vector(&inv, d);
      let found = match shape {
        Some(sid) => self.shapes.ray(sid, lo, ld).map(|(t, face, uv, n)| (t, Some(face), uv, n)),
        None => ray_box(&bounds, lo, ld).map(|(t, n)| (t, None, None, n)),
      };
      if let Some((t, face, uv, local_normal)) = found {
        let nm = normal_matrix(&world);
        let wn = transform_vector(&nm, local_normal);
        let l = (wn[0] * wn[0] + wn[1] * wn[1] + wn[2] * wn[2]).sqrt();
        let mut normal = if l > 0.0 { [wn[0] / l, wn[1] / l, wn[2] / l] } else { wn };
        // Face the ray, whichever side was struck.
        if normal[0] * d[0] + normal[1] * d[1] + normal[2] * d[2] > 0.0 {
          normal = [-normal[0], -normal[1], -normal[2]];
        }
        hits.push(Hit {
          node: self.id_of(i),
          distance: t,
          point: [origin[0] + d[0] * t, origin[1] + d[1] * t, origin[2] + d[2] * t],
          normal,
          face,
          uv,
        });
      }
    }
    hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
    Ok(hits)
  }

  /// Every shown node with bounds the volume touches, each with its
  /// deepest contact (point on the node's surface, the direction out of
  /// it, the depth along that direction). A node with a shape is tested
  /// per triangle in world space, so it holds under any transform; a node
  /// without one is its local box's twelve. Surfaces, not solids: a volume
  /// wholly inside a closed mesh with no triangle in reach touches
  /// nothing, the trimesh contract everywhere. Unordered; reads the index
  /// as of the last flush, like `raycast`.
  pub fn overlap(&mut self, volume: &Volume, filter: &QueryFilter) -> Result<Vec<Overlap>, String> {
    let root = self.filter_root(filter)?;
    let query = collide::Query::new(volume);
    let aabb = query.bounds(None);
    let mut candidates = Vec::new();
    self.bvh.query(&aabb, &mut |i| candidates.push(i));
    let mut out = Vec::new();
    for i in candidates {
      let Some((bounds, world, shape)) = self.collider(i, filter, root) else {
        continue;
      };
      let mut best: Option<collide::Contact> = None;
      node_triangles(&mut self.shapes, shape, &bounds, &world, &aabb, &mut |tri| {
        if let Some(c) = query.overlap_triangle(&tri) {
          if best.is_none_or(|b| c.2 > b.2) {
            best = Some(c);
          }
        }
      });
      if let Some((point, normal, depth)) = best {
        out.push(Overlap { node: self.id_of(i), point, normal, depth });
      }
    }
    Ok(out)
  }

  /// The volume moved by `motion`: every shown node with bounds it
  /// touches on the way, each at its first touch (time as a fraction of
  /// the motion, the touch point, the normal facing the volume), earliest
  /// first. A node already in contact reports time 0 while the motion
  /// closes in, and nothing while it leaves or slides along the contact.
  /// Same testing and index contract as `overlap`; a zero motion
  /// touches nothing.
  pub fn sweep(&mut self, volume: &Volume, motion: [f32; 3], filter: &QueryFilter) -> Result<Vec<Impact>, String> {
    let root = self.filter_root(filter)?;
    let query = collide::Query::new(volume);
    let aabb = query.bounds(Some(motion));
    let mut candidates = Vec::new();
    self.bvh.query(&aabb, &mut |i| candidates.push(i));
    let mut out = Vec::new();
    for i in candidates {
      let Some((bounds, world, shape)) = self.collider(i, filter, root) else {
        continue;
      };
      let mut first: Option<(f32, [f32; 3], [f32; 3])> = None;
      node_triangles(&mut self.shapes, shape, &bounds, &world, &aabb, &mut |tri| {
        if let Some(h) = query.sweep_triangle(motion, &tri) {
          if first.is_none_or(|f| h.0 < f.0) {
            first = Some(h);
          }
        }
      });
      if let Some((time, point, normal)) = first {
        out.push(Impact { node: self.id_of(i), time, point, normal });
      }
    }
    out.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
  }

  /// The filter's root as an arena index (None without one); a root that
  /// no longer resolves is the caller's error, not an empty answer.
  fn filter_root(&self, filter: &QueryFilter) -> Result<Option<u32>, String> {
    filter.root.map(|id| self.resolve(id)).transpose()
  }

  /// What the queries test a node by: its local box, world matrix and
  /// (resolvable) shape; None for a node the queries skip - not shown,
  /// leaving (a node animating out is a ghost: painted and nothing else),
  /// no bounds, or outside the filter.
  fn collider(&self, i: u32, filter: &QueryFilter, root: Option<u32>) -> Option<(Box3, Mat4, Option<ShapeId>)> {
    let n = &self.nodes[i as usize];
    if !n.alive || !n.shown || n.leaving {
      return None;
    }
    let bounds = n.bounds?;
    if filter.layers.is_some_and(|mask| n.layers & mask == 0) {
      return None;
    }
    if root.is_some_and(|r| !self.under(i, r)) {
      return None;
    }
    if filter.nodes.as_ref().is_some_and(|ids| !ids.contains(&self.id_of(i))) {
      return None;
    }
    if filter.target.is_some_and(|t| !self.lod_allows(i, t, false)) {
      return None;
    }
    // A shape id that no longer resolves (destroyed), or one naming no
    // triangles, falls back to the box, like a node that never had one.
    let shape = n.shape.filter(|&sid| self.shapes.has_triangles(sid));
    Some((bounds, n.world, shape))
  }

  /// Whether `i` is `root` or lies under it (the tree is shallow: a walk
  /// up the parents per candidate, no cached ancestry to keep in step).
  fn under(&self, mut i: u32, root: u32) -> bool {
    loop {
      if i == root {
        return true;
      }
      match self.nodes[i as usize].parent {
        Some(p) => i = p,
        None => return false,
      }
    }
  }

  /// Re-parent a node (None = make it a root). Errs on a cycle.
  pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let p = match parent {
      Some(pid) => Some(self.resolve(pid)?),
      None => None,
    };
    // A corpse is nobody's frame and nobody's child: re-parenting under
    // one would gate its free on a node that never leaves, and moving one
    // would tear it out of the frame its exit plays in.
    if self.nodes[i as usize].leaving {
      return Err(format!("spatial node {id} is leaving"));
    }
    if p.is_some_and(|p| self.nodes[p as usize].leaving) {
      return Err(format!("spatial node {} is leaving", parent.unwrap_or(0)));
    }
    if let Some(p) = p {
      let mut cursor = Some(p);
      while let Some(c) = cursor {
        if c == i {
          return Err(format!("spatial node {id} cannot be its own ancestor"));
        }
        cursor = self.nodes[c as usize].parent;
      }
    }
    if self.nodes[i as usize].parent == p {
      return Ok(());
    }
    if let Some(old) = self.nodes[i as usize].parent {
      self.nodes[old as usize].children.retain(|&c| c != i);
    }
    self.nodes[i as usize].parent = p;
    if let Some(p) = p {
      self.nodes[p as usize].children.push(i);
    }
    self.enqueue(i);
    Ok(())
  }

  /// Replace the local transform. The consumer compares before calling; an
  /// unchanged write still queues the node.
  pub fn set_transform(
    &mut self,
    id: NodeId,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
  ) -> Result<(), String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    n.position = position;
    n.rotation = rotation;
    n.scale = scale;
    n.local_dirty = true;
    self.enqueue(i);
    Ok(())
  }

  /// Declare (or with None clear) the node's transition config. With one
  /// set, `write_transform` animates instead of snapping. Clearing cancels
  /// the node's running tracks in place: it keeps its mid-flight transform,
  /// no settled events fire, and later writes snap. Replacing a config does
  /// not retroactively affect running tracks (element semantics).
  pub fn set_node_transition(&mut self, id: NodeId, config: Option<NodeTransitionConfig>) -> Result<(), String> {
    let i = self.resolve(id)?;
    match config {
      Some(c) => {
        self.transitions.configs.insert(id, c);
      }
      None => {
        self.transitions.configs.remove(&id);
        self.transitions.cancel_node(id);
      }
    }
    // A leaving node's gate is its declaration's exit set: a change to
    // it (a clear drops the tracks) may have emptied the gate.
    if self.nodes[i as usize].leaving {
      self.transitions.exit_checks.push(id);
    }
    Ok(())
  }

  /// Replace the local transform THROUGH the node's transition declaration:
  /// a component with a declared motion animates toward the written value
  /// (the write is a target; with a `delay` it is held that long first),
  /// one without snaps. Without a declaration the whole write snaps,
  /// exactly `set_transform`. A component matching its running track's
  /// target, its held write's target or its resting value is left alone -
  /// the full-TRS write shape re-sends unchanged components on every call.
  /// Returns whether anything changed (a track started or retargeted, a
  /// write held, or a snap moved the node) - the caller's frame-demand
  /// signal. A raw `set_transform` never consults or cancels tracks: a
  /// running track overwrites it at the next advance (last write wins, the
  /// producer rule).
  pub fn write_transform(
    &mut self,
    id: NodeId,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
  ) -> Result<bool, String> {
    let i = self.resolve(id)?;
    let Some(config) = self.transitions.configs.get(&id).cloned() else {
      let n = &self.nodes[i as usize];
      if n.position == position && n.rotation == rotation && n.scale == scale {
        return Ok(false);
      }
      self.set_transform(id, position, rotation, scale)?;
      return Ok(true);
    };
    let n = &self.nodes[i as usize];
    let writes = [
      (Component::Position, lanes3(n.position), lanes3(position)),
      (Component::Scale, lanes3(n.scale), lanes3(scale)),
      (Component::Rotation, n.rotation, rotation),
    ];
    let mut animated = false;
    let mut snapped = false;
    for (component, current, to) in writes {
      match config.motion_for(component) {
        Some(motion) => {
          let unchanged = targets_match(component, current, to) && !self.transitions.any_running(id, component);
          animated |= self.transitions.write(id, component, current, to, motion, unchanged);
        }
        None => {
          if !targets_match(component, current, to) {
            let n = &mut self.nodes[i as usize];
            match component {
              Component::Position => n.position = position,
              Component::Scale => n.scale = scale,
              Component::Rotation => n.rotation = rotation,
              // The weights register has its own write (`write_weights`).
              Component::Weights => {}
            }
            snapped = true;
          }
        }
      }
    }
    if snapped {
      let n = &mut self.nodes[i as usize];
      n.local_dirty = true;
      self.enqueue(i);
    }
    Ok(animated || snapped)
  }

  /// Stamp the animation clock (app-time ms, the paced timeline). Stamped
  /// once per frame before any frame work runs, so writes and the advance
  /// agree on time; pause/scale/step semantics ride in with the stamp.
  pub fn set_transition_now(&mut self, now_ms: f64) {
    self.transitions.now_ms = now_ms;
    // Stagger indices are per frame: each stamp opens a fresh count.
    self.transitions.stagger_counts.clear();
  }

  /// The nearest ancestor declaring `stagger_ms` (its arena index and
  /// spacing), None without one.
  fn stagger_group_of(&self, i: u32) -> Option<(u32, f32)> {
    let mut cursor = self.nodes[i as usize].parent;
    while let Some(p) = cursor {
      let pid = self.id_of(p);
      if let Some(stagger_ms) = self.transitions.configs.get(&pid).and_then(|c| c.stagger_ms) {
        return Some((p, stagger_ms));
      }
      cursor = self.nodes[p as usize].parent;
    }
    None
  }

  /// The extra delay a stagger group imposes on this node's lifecycle
  /// event (enter when `exit` is false, exit when true): `index *
  /// stagger_ms` under the nearest ancestor declaring `stagger_ms`, zero
  /// without one. Counting is per group per frame in occurrence order:
  /// creation order for enters (JSX order for template children), tree
  /// order for exits (`start_staggered_exits` assigns those).
  fn stagger_delay_for(&mut self, i: u32, exit: bool) -> f32 {
    match self.stagger_group_of(i) {
      Some((g, stagger_ms)) => self.transitions.stagger_index(self.id_of(g), exit) as f32 * stagger_ms,
      None => 0.0,
    }
  }

  /// Advance every running track to the stamped clock, writing the
  /// interpolated TRS through the ordinary snap path (nodes queue; the
  /// next flush propagates). Held writes whose delay expired apply first,
  /// as of their scheduled time. Settled tracks land the target exactly
  /// and report via `take_settled_transitions`, except on a leaving node,
  /// whose settles feed its free gate instead (`check_exit`, run after the
  /// pass; a freed node lands in `take_freed`). Tracks of freed nodes drop
  /// silently. Returns whether any track still runs or any write still
  /// waits - the embedder's signal to keep requesting frames. A repeated
  /// call at an unchanged clock (the paused path) writes nothing.
  pub fn advance_transitions(&mut self) -> bool {
    self.start_enter_transitions();
    let now = self.transitions.now_ms;
    let mut exit_checks = std::mem::take(&mut self.transitions.exit_checks);
    self.start_staggered_exits(&mut exit_checks);
    if self.transitions.is_empty() && exit_checks.is_empty() {
      return false;
    }
    // Due writes apply exactly as a write this frame would, from the
    // component's present value; state may have shifted during the hold
    // (the node died), and a write that no longer applies is dropped.
    for w in self.transitions.take_due(now) {
      let Ok(i) = self.resolve(w.node) else {
        continue;
      };
      let n = &self.nodes[i as usize];
      let current = match w.component {
        Component::Position => lanes3(n.position),
        Component::Scale => lanes3(n.scale),
        Component::Rotation => n.rotation,
        // Held weights writes live in their own list (take_due_weights).
        Component::Weights => continue,
      };
      let running = self.transitions.apply(w.node, w.component, current, w.to, w.spec, w.at_ms);
      // A due exit write that starts nothing (the value already there)
      // may have been the last thing keeping its node around.
      if !running && n.leaving {
        exit_checks.push(w.node);
      }
    }
    for w in self.transitions.take_due_weights(now) {
      let Ok(i) = self.resolve(w.node) else {
        continue;
      };
      let current = self.nodes[i as usize].weights.clone();
      let running = self.transitions.retarget_weights(w.node, &current, &w.to, w.spec, w.at_ms);
      if !running && self.nodes[i as usize].leaving {
        exit_checks.push(w.node);
      }
    }
    let mut linear = std::mem::take(&mut self.transitions.linear);
    linear.retain_mut(|track| {
      let Ok(i) = self.resolve(track.node) else {
        return false;
      };
      let (value, settled) = track.advance(now);
      let n = &mut self.nodes[i as usize];
      let slot = match track.component {
        Component::Position => &mut n.position,
        Component::Scale => &mut n.scale,
        // Rotation and weights tracks live in their own lists.
        Component::Rotation | Component::Weights => unreachable!("only position and scale tracks are linear"),
      };
      if *slot != value {
        *slot = value;
        n.local_dirty = true;
        self.enqueue(i);
      }
      if settled {
        if self.nodes[i as usize].leaving {
          exit_checks.push(track.node);
        } else {
          self.transitions.settled.push((track.node, track.component));
        }
        return false;
      }
      true
    });
    linear.append(&mut self.transitions.linear);
    self.transitions.linear = linear;
    let mut rotation = std::mem::take(&mut self.transitions.rotation);
    rotation.retain_mut(|track| {
      let Ok(i) = self.resolve(track.node) else {
        return false;
      };
      let (value, settled) = track.advance(now);
      let n = &mut self.nodes[i as usize];
      if n.rotation != value {
        n.rotation = value;
        n.local_dirty = true;
        self.enqueue(i);
      }
      if settled {
        if self.nodes[i as usize].leaving {
          exit_checks.push(track.node);
        } else {
          self.transitions.settled.push((track.node, Component::Rotation));
        }
        return false;
      }
      true
    });
    rotation.append(&mut self.transitions.rotation);
    self.transitions.rotation = rotation;
    let mut weights = std::mem::take(&mut self.transitions.weights);
    let mut value = std::mem::take(&mut self.weights_scratch);
    weights.retain_mut(|track| {
      let Ok(i) = self.resolve(track.node) else {
        return false;
      };
      let settled = track.advance(now, &mut value);
      if self.nodes[i as usize].weights != value {
        let n = &mut self.nodes[i as usize];
        n.weights.clear();
        n.weights.extend_from_slice(&value);
        self.mark_weights(i);
      }
      if settled {
        if self.nodes[i as usize].leaving {
          exit_checks.push(track.node);
        } else {
          self.transitions.settled.push((track.node, Component::Weights));
        }
        return false;
      }
      true
    });
    self.weights_scratch = value;
    weights.append(&mut self.transitions.weights);
    self.transitions.weights = weights;
    exit_checks.sort_unstable();
    exit_checks.dedup();
    for id in exit_checks {
      if let Ok(i) = self.resolve(id) {
        self.check_exit(i);
      }
    }
    !self.transitions.is_empty()
  }

  /// Enter animations: a node created since the last advance whose
  /// declaration carries `from` endpoints snaps those components to them
  /// and animates (or, with a delay on the endpoint, waits, then animates)
  /// toward the transform it holds now - the created one, or the target of
  /// a write the creating tick already made. Runs first thing in the
  /// advance, so the creating tick may set the declaration and the pose in
  /// any order, and the node's first flushed transform is the `from` one.
  /// Once per node: a freed node is skipped, one already leaving (let go
  /// of in its creating tick) spends its enter and leaves from where it
  /// is, and creation is the only way onto the queue.
  fn start_enter_transitions(&mut self) {
    let now = self.transitions.now_ms;
    for id in std::mem::take(&mut self.transitions.entering) {
      let Ok(i) = self.resolve(id) else {
        continue;
      };
      if self.nodes[i as usize].leaving {
        continue;
      }
      let Some(config) = self.transitions.configs.get(&id).cloned() else {
        continue;
      };
      let n = &self.nodes[i as usize];
      let enters = [
        (
          Component::Position,
          lanes3(n.position),
          config.position.and_then(|e| e.from.map(|f| (lanes3(f.value), f.motion))),
        ),
        (Component::Scale, lanes3(n.scale), config.scale.and_then(|e| e.from.map(|f| (lanes3(f.value), f.motion)))),
        (Component::Rotation, n.rotation, config.rotation.and_then(|e| e.from.map(|f| (f.value, f.motion)))),
      ];
      let mut snapped = false;
      // One stagger index per node, shared by all its entering components,
      // so a multi-component enter moves as one item of the cascade.
      let mut stagger: Option<f32> = None;
      for (component, held, enter) in enters {
        let Some((from, motion)) = enter else { continue };
        if stagger.is_none() {
          stagger = Some(self.stagger_delay_for(i, false));
        }
        let target = self.transitions.take_target(id, component).unwrap_or(held);
        if targets_match(component, from, target) {
          continue;
        }
        let n = &mut self.nodes[i as usize];
        match component {
          Component::Position => n.position = [from[0], from[1], from[2]],
          Component::Scale => n.scale = [from[0], from[1], from[2]],
          Component::Rotation => n.rotation = from,
          // The weights enter follows below, on its own lanes.
          Component::Weights => {}
        }
        snapped = true;
        let delay_ms = motion.delay_ms + stagger.unwrap_or(0.0);
        if delay_ms > 0.0 {
          let at_ms = now + delay_ms as f64;
          self.transitions.schedule(PendingWrite { node: id, component, to: target, spec: motion.spec, at_ms });
        } else {
          self.transitions.apply(id, component, from, target, motion.spec, now);
        }
      }
      if snapped {
        self.nodes[i as usize].local_dirty = true;
        self.enqueue(i);
      }
      // The weights register enters the same way: snapped to `from`,
      // animating to what it holds (or a write already heads for).
      if let Some(f) = config.weights.and_then(|e| e.from) {
        if stagger.is_none() {
          stagger = Some(self.stagger_delay_for(i, false));
        }
        let held = self.nodes[i as usize].weights.clone();
        let target = self.transitions.take_weights_target(id).unwrap_or(held);
        let len = f.value.len().max(target.len());
        let mut from = f.value.clone();
        from.resize(len, 0.0);
        let mut target = target;
        target.resize(len, 0.0);
        if from == target {
          continue;
        }
        self.nodes[i as usize].weights = from.clone();
        self.mark_weights(i);
        let delay_ms = f.motion.delay_ms + stagger.unwrap_or(0.0);
        if delay_ms > 0.0 {
          let at_ms = now + delay_ms as f64;
          self.transitions.schedule_weights(PendingWeights { node: id, to: target, spec: f.motion.spec, at_ms });
        } else {
          self.transitions.retarget_weights(id, &from, &target, f.motion.spec, now);
        }
      }
    }
  }

  /// The (node, component) pairs whose tracks settled since the last drain
  /// (the onTransitionEnd feed). Cancelled tracks never appear.
  pub fn take_settled_transitions(&mut self) -> Vec<(NodeId, Component)> {
    std::mem::take(&mut self.transitions.settled)
  }

  pub fn set_visible(&mut self, id: NodeId, visible: bool) -> Result<(), String> {
    let i = self.resolve(id)?;
    if self.nodes[i as usize].visible == visible {
      return Ok(());
    }
    self.nodes[i as usize].visible = visible;
    self.enqueue(i);
    Ok(())
  }

  /// Attach the node's draw sink on the sink's target, replacing the one
  /// it had there. A new sink's entry is assumed switched OFF (instance
  /// count 0, how the 3d package adds entries); the next flush turns it on
  /// with a params write if the node is shown. The node re-queues, so its
  /// other sinks get a params rewrite in that flush too (a queued node
  /// recomputes unconditionally, the reparent rule). A node's draw sinks
  /// share one sort key: a sink bound while the node has others takes
  /// theirs, and `set_sink_order` changes them all.
  pub fn bind_sink(&mut self, id: NodeId, mut sink: DrawSink) -> Result<(), String> {
    let i = self.resolve(id)?;
    self.bind_seq += 1;
    let seq = self.bind_seq;
    let sinks = &mut self.nodes[i as usize].sinks;
    sinks.retain(|b| b.sink.target != sink.target);
    if let Some(other) = sinks.first() {
      sink.order = other.sink.order;
    }
    sinks.push(BoundSink { sink, seq, entry_on: false, fresh: true, fade: [1.0, 1.0] });
    if let Some(sort) = self.draw_sorts.get_mut(&sink.target) {
      sort.dirty = true;
    }
    self.enqueue(i);
    Ok(())
  }

  /// Remove the node's draw sink on `target` (or every draw sink with
  /// None). Issues no write: the entries are the consumer's to remove.
  pub fn unbind_sink(&mut self, id: NodeId, target: Option<u64>) -> Result<(), String> {
    let i = self.resolve(id)?;
    let sorts = &mut self.draw_sorts;
    self.nodes[i as usize].sinks.retain(|b| {
      let keep = target.is_some_and(|t| t != b.sink.target);
      if !keep {
        if let Some(sort) = sorts.get_mut(&b.sink.target) {
          sort.dirty = true;
        }
      }
      keep
    });
    self.enqueue(i);
    Ok(())
  }

  /// Re-key every draw sink of the node (a material swap, a render_order
  /// change): its sorted targets re-sort at the next flush.
  pub fn set_sink_order(&mut self, id: NodeId, order: DrawOrder) -> Result<(), String> {
    let i = self.resolve(id)?;
    for b in &mut self.nodes[i as usize].sinks {
      if b.sink.order != order {
        b.sink.order = order;
        if let Some(sort) = self.draw_sorts.get_mut(&b.sink.target) {
          sort.dirty = true;
        }
      }
    }
    Ok(())
  }

  /// Change the "on" count of every draw sink (an instanced mesh's record
  /// count). Entries currently on get the new count through `out` at once;
  /// returns whether any did.
  pub fn set_sink_count(&mut self, id: NodeId, count: u32, out: &mut dyn SinkWriter) -> Result<bool, String> {
    let i = self.resolve(id)?;
    let n = &mut self.nodes[i as usize];
    if n.sinks.is_empty() {
      return Err(format!("spatial node {id} has no draw sink"));
    }
    let mut wrote = false;
    n.sinks.retain_mut(|b| {
      b.sink.count = count;
      if !b.entry_on {
        return true;
      }
      let landed = out.write_count(b.sink.target, b.sink.draw, count);
      wrote |= landed;
      landed
    });
    Ok(wrote)
  }

  /// Effective visibility as of the last flush.
  pub fn shown(&self, id: NodeId) -> Result<bool, String> {
    Ok(self.nodes[self.resolve(id)? as usize].shown)
  }

  /// The node's world matrix as the tree stands NOW, pending writes
  /// included: a dirty chain is composed on the fly without clearing any
  /// flag, so the next flush still sees it. O(depth) when something above
  /// is dirty, a copy otherwise.
  pub fn world(&self, id: NodeId) -> Result<Mat4, String> {
    let i = self.resolve(id)?;
    let mut chain = Vec::new();
    let mut cursor = Some(i);
    let mut top_dirty = None;
    while let Some(c) = cursor {
      let n = &self.nodes[c as usize];
      if n.queued || n.local_dirty {
        top_dirty = Some(chain.len());
      }
      chain.push(c);
      cursor = n.parent;
    }
    let Some(top) = top_dirty else {
      return Ok(self.nodes[i as usize].world);
    };
    let mut world = match self.nodes[chain[top] as usize].parent {
      Some(p) => self.nodes[p as usize].world,
      None => IDENTITY,
    };
    for &c in chain[..=top].iter().rev() {
      let n = &self.nodes[c as usize];
      let local = if n.local_dirty { compose(n.position, n.rotation, n.scale) } else { n.local };
      world = multiply(world, local);
    }
    Ok(world)
  }

  /// Recompute every queued subtree and hand the sink writes to `out`. A
  /// group whose write did not land is dropped with it (see `SinkWriter`);
  /// the slots still naming it stage nothing, so it never writes again.
  pub fn flush(&mut self, out: &mut dyn SinkWriter) {
    self.flush_id += 1;
    self.refit_shaped();
    if !self.queue.is_empty() {
      let queue = std::mem::take(&mut self.queue);
      for &i in &queue {
        if !self.nodes[i as usize].alive || self.has_queued_ancestor(i) {
          continue;
        }
        let (parent_world, parent_shown) = match self.nodes[i as usize].parent {
          Some(p) => (self.nodes[p as usize].world, self.nodes[p as usize].shown),
          None => (IDENTITY, true),
        };
        let parent_scope = match self.nodes[i as usize].parent {
          Some(p) => self.nodes[p as usize].lod_scope,
          None => None,
        };
        self.recompute(i, &parent_world, false, parent_shown, parent_scope, out);
      }
      for &i in &queue {
        self.nodes[i as usize].queued = false;
      }
    }
    if !self.lod_dirty.is_empty() || !self.lod_moved.is_empty() {
      self.lod_pass();
    }
    if !self.touched.is_empty() || !self.frustum_dirty.is_empty() {
      self.cull_pass(out);
    }
    if self.draw_sorts.values().any(|s| s.dirty || s.view_moved) {
      self.order_pass(out);
    }
    // Shared params changed by the walk, an unbind or a destroy go out
    // once per flush, whole; a group nothing references any more goes
    // with its last write.
    self.shared.retain(|(target, name), group| {
      let mut landed = true;
      if group.dirty {
        group.dirty = false;
        landed = out.write_shared(*target, name, &group.values);
      }
      landed && group.refs > 0
    });
    // Instance staging publishes the same way: one coalesced dirty range
    // per buffer per flush, a group dropping with its last write.
    self.instances.retain(|buffer, group| {
      let mut landed = true;
      if let Some((lo, hi)) = group.dirty.take() {
        landed = out.write_instances(*buffer, lo as u32, hi as u32, &group.values);
      }
      landed && group.refs > 0
    });
    // Palette staging publishes whole per texture. Anchored rows relativize
    // here, at the flush's end: the anchor's world is fresh (an anchor move
    // restaged every row, since it is an ancestor of every bound node), and
    // one inverse covers the palette. A dead anchor falls back to identity
    // (the consumer tears joints down before their model root, so live
    // sinks never see it).
    // Weights rows stage from the registers, beside the matrix rows the
    // walk staged.
    self.stage_weights();
    if self.palettes.values().any(|g| g.dirty || g.refs == 0) {
      let mut palettes = std::mem::take(&mut self.palettes);
      let mut scratch: Vec<f32> = Vec::new();
      palettes.retain(|texture, group| {
        let mut landed = true;
        if group.dirty {
          group.dirty = false;
          landed = match group.anchor {
            None => out.write_texture(*texture, &group.values),
            Some(a) => {
              let inv = match self.resolve(a) {
                Ok(i) => invert_affine(&self.nodes[i as usize].world),
                Err(_) => IDENTITY,
              };
              scratch.clear();
              for row in group.values.chunks(16) {
                let m: Mat4 = row.try_into().expect("palette rows are 16 floats");
                scratch.extend_from_slice(&multiply(inv, m));
              }
              out.write_texture(*texture, &scratch)
            }
          };
        }
        landed && group.refs > 0
      });
      self.palettes = palettes;
    }
  }

  fn has_queued_ancestor(&self, i: u32) -> bool {
    let mut cursor = self.nodes[i as usize].parent;
    while let Some(c) = cursor {
      if self.nodes[c as usize].queued {
        return true;
      }
      cursor = self.nodes[c as usize].parent;
    }
    false
  }

  fn recompute(
    &mut self,
    i: u32,
    parent_world: &Mat4,
    parent_changed: bool,
    parent_shown: bool,
    parent_scope: Option<(u32, u32)>,
    out: &mut dyn SinkWriter,
  ) {
    let n = &mut self.nodes[i as usize];
    let mut changed = parent_changed;
    if n.local_dirty {
      n.local = compose(n.position, n.rotation, n.scale);
      n.local_dirty = false;
      changed = true;
    }
    // A queued node recomputes unconditionally: a re-parent or a sink
    // change leaves the matrices alone but the subtree still has to be
    // revisited (a new parent means a new world matrix).
    if changed || n.queued {
      n.world = multiply(*parent_world, n.local);
      changed = true;
    }
    let shown = parent_shown && n.visible;
    let scope = n.lod_level.or(parent_scope);
    let touched = changed || shown != n.shown || n.queued || scope != n.lod_scope;
    n.shown = shown;
    n.lod_scope = scope;
    if changed && !n.lod_check && (n.lod.is_some() || n.records.len() > 1) {
      n.lod_check = true;
      self.lod_moved.push(i);
    }
    let refit = n.bounds.is_some() && (changed || n.leaf.is_none());
    if changed || n.world_box.is_none() {
      n.world_box = n.cull_bounds.or(n.bounds).map(|b| world_box(&b, &n.world));
    }

    if changed {
      // Disjoint borrows: the node's slots read, the shared groups written.
      let shared = &mut self.shared;
      for slot in &n.slots {
        let v = match slot.projection {
          Projection::Direction(local) => {
            let v = transform_vector(&n.world, local);
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 0.0 {
              [v[0] / l, v[1] / l, v[2] / l]
            } else {
              [0.0; 3]
            }
          }
          Projection::Position => [n.world[12], n.world[13], n.world[14]],
        };
        let at = slot.index as usize * 3;
        if let Some(group) = shared.get_mut(&(slot.target, slot.name.clone())) {
          if group.values[at..at + 3] != v {
            group.values[at..at + 3].copy_from_slice(&v);
            group.dirty = true;
          }
        }
      }
      let palettes = &mut self.palettes;
      for slot in &n.texture_slots {
        if let Some(group) = palettes.get_mut(&slot.texture) {
          let m = multiply(n.world, slot.post);
          let at = slot.row as usize * 16;
          if group.values[at..at + 16] != m {
            group.values[at..at + 16].copy_from_slice(&m);
            group.dirty = true;
          }
        }
      }
    }
    let world = self.nodes[i as usize].world;
    if changed && !self.nodes[i as usize].cull_owners.is_empty() {
      // A group member moved: the owners re-test against the fresh union.
      let owners = self.nodes[i as usize].cull_owners.clone();
      for o in owners {
        if let Ok(j) = self.resolve(o) {
          self.touch(j);
        }
      }
    }
    if touched && !self.nodes[i as usize].sinks.is_empty() && !self.nodes[i as usize].queued_touch {
      self.nodes[i as usize].queued_touch = true;
      self.touched.push(i);
    }
    // The inverse-transpose is one matrix however many sinks ask for it.
    let mut normal: Option<Mat4> = None;
    // Entries that are on AND staying on get their fresh matrix here; the
    // switch itself (visibility and the frustum) is the cull pass's, after
    // the walk, so a group's boxes are all current when it is decided. An
    // entry that is off, or about to go off, remembers it owes a params
    // write for when it turns on (a group member not yet recomputed can
    // make the test here say off while the pass says on: the owed write
    // covers that). A sink whose entry is gone (a write that did not
    // land) is released here: the entry was the consumer's to remove, and
    // it did.
    let mut k = 0;
    while k < self.nodes[i as usize].sinks.len() {
      let b = self.nodes[i as usize].sinks[k];
      let staying_on =
        shown && b.entry_on && self.frustum_allows(i, b.sink.target) && self.lod_allows(i, b.sink.target, b.sink.fade);
      let sinks = &mut self.nodes[i as usize].sinks;
      if staying_on && (changed || b.fresh) {
        if b.sink.normal && normal.is_none() {
          normal = Some(normal_matrix(&world));
        }
        sinks[k].fresh = false;
        if !out.write_params(b.sink.target, b.sink.draw, &world, if b.sink.normal { normal.as_ref() } else { None }) {
          sinks.remove(k);
          continue;
        }
      } else if changed {
        sinks[k].fresh = true;
      }
      k += 1;
    }
    let level = self.nodes[i as usize].record_level as usize;
    let record = self.nodes[i as usize].records.get(level).copied();
    let record_on = self.nodes[i as usize].record_on;
    if let Some(rec) = record {
      if shown && (changed || !record_on) {
        let m = self.anchored(rec.buffer, &world);
        match rec.projection {
          InstanceProjection::Pose2D => self.stage_record(&rec, &pose2d(&m)),
          InstanceProjection::Matrix => self.stage_record(&rec, &m),
        }
        self.nodes[i as usize].record_on = true;
      } else if !shown && record_on {
        self.stage_record(&rec, rec.projection.hidden());
        self.nodes[i as usize].record_on = false;
      }
    }
    if refit {
      self.refit_leaf(i);
    }
    let mut k = 0;
    while k < self.nodes[i as usize].children.len() {
      let c = self.nodes[i as usize].children[k];
      self.recompute(c, &world, changed, shown, scope, out);
      k += 1;
    }
  }
}
