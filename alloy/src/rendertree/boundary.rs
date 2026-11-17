// Repaint-boundary caching and compositing: the retained paint results
// (PaintCache and its snapshot/shader halves), what a boundary caller hoists
// out of them (Hoist), and the composite paths that put a cached result back
// into the tree - draw_cached_recording for Recording boundaries,
// BoundaryComposite for snapshot boundaries. The walk itself
// (composite::build_recursive / record_node) stays in composite.rs and calls
// in here at each boundary node.

use crate::renderer::prelude::*;
use crate::renderer::{
  ClipOperation, DisplayList, DisplayListBuilder, Matrix, Paint, Point, Rect, Size, Texture, TextureSampling,
};

use crate::rendertree::composite::{
  apply_clip, apply_scroll, effect_paint, emit_backdrop, record_node, service_captures_under_cache, snapped_own,
  view_filter, view_opacity, CLIP_INF,
};
use crate::rendertree::{grid, text, BuildContext, Element, ElementKind, FilterState, RenderTree, Vector};

/// The narrowest side a snapshot raster made on an unrounded grid takes,
/// in texels of its density: a positive side below one texel is one,
/// never nothing (whole_texels).
const RASTER_MIN_TEXELS: f32 = 1.0;

/// How far above a whole texel count a snapshot canvas still counts as
/// that count, in texels, at the least: float noise of `box / scale *
/// scale` (60.000004 for 60) must add no texel, which would be a
/// transparent column the stretched quad samples (canvas_texels).
const TEXEL_CEIL_EPSILON: f32 = 1e-3;

/// The same tolerance as a fraction of the canvas's texels, where that is
/// more: a box snapped on a grid whose scale is held at the raster's
/// density (within text's SCALE_HELD_EPSILON of it, 1e-5) is off from its
/// whole texels by up to that fraction of them, so this must exceed it. It
/// crops at most this fraction of a texel per texel of a real sliver.
const TEXEL_CEIL_RELATIVE: f32 = 1e-4;

// What a boundary caller applies itself at composite time, and record_node
// therefore leaves out of the cached content. The record order is matrix,
// clip, scroll, fit, children; a hoist always covers a prefix of the first
// three (a hoisted scroll requires a hoisted clip, otherwise the
// composite-time scroll translate would move a recorded clip that must stay
// put in viewport space; a design-size fit is never hoisted - it is content).
// A hoisted matrix is carried: the View's own matrix as the caller snapped
// it (composite::snapped_own), so the record walk folds the same matrix
// into its maps that the composite applies.
#[derive(Clone, Copy)]
pub(super) enum Hoist {
  /// Record everything (non-boundary nodes, non-View boundaries).
  None,
  /// The caller applies the View's matrix; clip and scroll stay recorded.
  /// Snapshot boundaries use this: their raster must bake clip and scroll,
  /// since the texture holds only the pixels visible at rasterize time.
  Transform(Matrix),
  /// The caller applies matrix, clip and scroll; the cache holds the fit and
  /// children only. Recording boundaries use this, making the cache reusable
  /// under scroll writes as well as transform writes (see Damage::Scroll).
  Full(Matrix),
}

/// A boundary's retained paint result, in node-local coordinates.
pub enum PaintCache {
  Recording(RecordingCache),
  Snapshot(SnapshotCache),
}

/// A Recording boundary's retained recording, plus what it knows about the
/// backdrop panels baked inside it. A replayed recording re-runs its baked
/// backdrop save_layers against the live window every frame, but on reuse
/// frames the walk does not enter the subtree, so the panels cannot push
/// their own damage-widening regions; the boundary pushes one conservative
/// region in their place (build_recursive's cached leg). A snapshot cache
/// needs no counterpart: its baked backdrop sampled the boundary's
/// offscreen at raster time, so window damage never changes its pixels.
pub struct RecordingCache {
  pub dl: DisplayList,
  pub backdrops: BakedBackdrops,
  /// The density its content made for one scale (text layers, floored
  /// strokes) was made for when recorded (the grid scale inside the
  /// boundary's own matrix, or a running zoom's end), which every replay
  /// judges the current scale against (composite.rs, text::raster_density).
  /// None under a rotation or a 3d transform, where there is no grid and
  /// nothing re-records for scale.
  pub grid_scale: Option<f32>,
  /// Whether the record walk drew content made for one scale, nested
  /// recordings included: text layers, and the stroke of every visibly
  /// painted rect that takes its geometry from the frame, whether the
  /// device-pixel floor changed its width or not
  /// (okf/done/border-width-device-pixels.md). A recording with neither
  /// replays crisply at any scale and never re-records for one.
  pub scale_bound: bool,
  /// Whether the grid it was recorded on (inside the boundary's own matrix)
  /// had one device width for a stroke (grid::floors_strokes). Content
  /// made for one scale that was recorded where no stroke could be floored
  /// (a rotation, a scale on one axis) holds declared widths, and the
  /// density compare does not see the frame come upright at the same
  /// longer-axis scale: a replay on a grid that floors re-records it once
  /// (composite.rs).
  pub floors_strokes: bool,
  /// Whether the rounding to the device grid was off when it was recorded
  /// (a running track moved it, or `pixelSnap={false}`): its interior is
  /// then unrounded, and a replay with the rounding back re-records it once
  /// (composite.rs). The density compare cannot catch this: a zoom records
  /// at its end density on a mid-motion grid, and the density still
  /// matches at rest.
  pub unrounded: bool,
  /// Whether it was recorded under `pixelSnap={false}`; a replay under the
  /// other setting re-records it.
  pub pixel_snap_off: bool,
  /// Whether its record walk drew content made mid-motion that must be
  /// made again when the scale rests (BuildContext::rest_pending moved):
  /// only inside a snapshot raster, where the texts do not wait for
  /// themselves. It is re-recorded once at rest (composite.rs), under the
  /// snapshot's re-render for rest at the latest.
  pub rest_pending: bool,
  /// The box it was recorded at (the walk's `ctx.size`: the snapped border
  /// box, the inherited frame for a detached boundary), which its content
  /// is laid out at (a design-size fit, detached children filling it). At
  /// the density it was recorded at, a replay at another box re-records it
  /// once (composite.rs): a move that took a far edge onto another device
  /// pixel. Under another density the box is a new logical size at every
  /// scale, and the recording replays there by design.
  pub size: Size,
}

/// The backdrop panels inside a Recording cache, summarized from the
/// regions its record walk pushed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BakedBackdrops {
  /// No panels inside; reuse frames push nothing.
  None,
  /// Panels inside, with the largest blur reach among them. Reuse frames
  /// push the boundary's current window-space subtree extent (which
  /// contains every panel) carrying this reach, so damage within reach of
  /// a panel widens to cover the boundary - conservative, never stale.
  Reach(f32),
  /// Some panel's region was unmappable at record time (a non-2D
  /// transform inside the recording); reuse frames push the same
  /// unmappable marker, degrading the resolve to full damage exactly like
  /// the record did.
  Unmappable,
}

impl BakedBackdrops {
  /// Summarize the region entries a boundary's record walk pushed.
  pub(super) fn summarize(regions: &[crate::rendertree::BackdropRegion]) -> Self {
    if regions.is_empty() {
      return BakedBackdrops::None;
    }
    let mut reach = 0.0f32;
    for entry in regions {
      match entry {
        None => return BakedBackdrops::Unmappable,
        Some((_, r)) => reach = reach.max(*r),
      }
    }
    BakedBackdrops::Reach(reach)
  }
}

/// A snapshot boundary's retained rasterization. It remembers the logical
/// box its raster was laid out at (the key's box, which the quad stretches
/// onto this frame's box) and the scale it was rasterized at: pixels are
/// resolution-dependent, so a mismatch forces re-rasterization even when
/// nothing inside the subtree changed. Invalidation marks it stale
/// (`valid: false`) instead of dropping it: the pixels are worthless but the
/// texture allocation is still exactly the right size, so the next raster
/// re-renders into it instead of reallocating (see boundary::snapshot_node).
/// All storage is exact-size; with an unchanged canvas the allocation is
/// reusable across shader declaration changes in either direction.
pub struct SnapshotCache {
  pub texture: Texture,
  pub width: f32,
  pub height: f32,
  pub scale: f32,
  pub valid: bool,
  /// The node's layout size (unsnapped) when it was rasterized, beside the
  /// box above. Under a running track the node's box is that layout size
  /// and at rest it is the snapped box, which is a new logical size at
  /// every scale of the grid, so keyed on the box alone every snapshot
  /// under a track would re-rasterize when the track starts and again when
  /// it ends, and every snapshot under a moving ancestor scale at every
  /// frame: while the rounding is off (a track, `pixelSnap={false}`), while
  /// the raster is composited stretched, and while this frame's box takes
  /// the same texels, the compare is on this size and the scale, and the
  /// key keeps the cached box. A raster made under a running track holds
  /// its layout box in whole texels, so it is kept through this compare,
  /// never through a float compare of the box. The quad draws at this
  /// frame's box either way (snapshot_node).
  pub layout: Size,
  /// Whether it was rasterized under `pixelSnap={false}`; under the other
  /// setting its interior holds the wrong rounding and it is stale.
  pub pixel_snap_off: bool,
  /// Whether its raster holds content made mid-motion that must be made
  /// again when the scale rests (BuildContext::rest_pending moved during
  /// its record walk: an unhinted text, a recording waiting for its own
  /// look), which the snapshot re-renders in place, for rest, once it is at
  /// rest (snapshot_node).
  pub rest_pending: bool,
  /// The shader half, present while a boundary shader is declared (see
  /// `View::set_shader`); its output is composited in place of `texture`.
  pub shaded: Option<ShadedCache>,
}

impl SnapshotCache {
  /// The dimensions of its textures, in texels: those of the key it was
  /// rasterized for, through the derivation every raster uses
  /// (SnapshotKey::texture_dims), so a reader cannot diverge from it.
  pub(crate) fn texture_dims(&self) -> (u32, u32) {
    let outset = self.shaded.as_ref().map_or(0.0, |sc| sc.outset);
    SnapshotKey { width: self.width, height: self.height, scale: self.scale, outset }.texture_dims()
  }
}

/// The boundary shader's cache: the pass output composited in place of the
/// raw snapshot, the outset the canvas was rasterized with (it joins the
/// validity key - a different outset means different storage), and, with
/// `previous` declared, the prior rasterization retained as the pass's
/// `uPrevious` input.
pub struct ShadedCache {
  pub output: Texture,
  pub outset: f32,
  pub history: Option<Texture>,
}

/// The identity of a snapshot boundary's storage: the logical box, the
/// display scale the pixels are rasterized at, and the shader canvas outset
/// (zero without a shader; a plain texture counts as outset zero). All
/// storage is exact-size, so every form of reuse - compositing a cached
/// texture, re-rendering into retained storage - requires the whole key to
/// match.
struct SnapshotKey {
  width: f32,
  height: f32,
  scale: f32,
  outset: f32,
}

impl SnapshotKey {
  /// The rasterization canvas in logical px: the box grown by the outset on
  /// every side.
  fn canvas(&self) -> Size {
    Size::new(self.width + 2.0 * self.outset, self.height + 2.0 * self.outset)
  }

  /// The exact-size texture dimensions for the canvas at this scale
  /// (canvas_texels per axis). The one derivation of a snapshot's
  /// dimensions (SnapshotCache::texture_dims reads it too).
  fn texture_dims(&self) -> (u32, u32) {
    let canvas = self.canvas();
    (canvas_texels(canvas.width, self.scale), canvas_texels(canvas.height, self.scale))
  }

  /// Whether retained storage was allocated for exactly this key.
  fn matches(&self, snap: &SnapshotCache) -> bool {
    snap.width == self.width
      && snap.height == self.height
      && snap.scale == self.scale
      && snap.shaded.as_ref().map_or(0.0, |sc| sc.outset) == self.outset
  }
}

/// How a snapshot boundary's result is composited at its place in the tree:
/// the hoisted transform, the backdrop layer, and the effect-carrying quad
/// (or the inline fallback), built once per visit so every cache state -
/// reused, re-rendered, freshly rasterized, failed - draws through the same
/// code. Group opacity and the view filter ride on the quad paint (white
/// keeps the texture's colors, the alpha fades it, the filters transform the
/// draw), so the texture itself stays effect-free and survives opacity and
/// filter writes.
struct BoundaryComposite<'e> {
  element: &'e Element,
  /// The View's own box transform, hoisted out of the raster
  /// (Hoist::Transform); None for non-View boundaries.
  own: Option<Matrix>,
  /// The inherited frame, for the backdrop bounds of a detached View.
  frame: Size,
  /// The content's region of the (ceil-padded) texture, in texture px: the
  /// key's canvas, the box the texture was made for.
  src: Rect,
  /// The quad in logical px: this frame's box grown by the key's outset,
  /// at the node's paint offset. It can differ from the key's box, which
  /// is then stretched onto it.
  dst: Rect,
  quad_paint: Option<Paint>,
  opacity: f32,
  filter: Option<&'e FilterState>,
  scale: f32,
  /// This frame's box over the key's, per axis: what the composite
  /// stretches the key's content box by to land it on this frame's box.
  stretch: (f32, f32),
}

impl<'e> BoundaryComposite<'e> {
  fn new(
    element: &'e Element,
    own: Option<Matrix>,
    frame: Size,
    offset: (f32, f32),
    painted: Size,
    key: &SnapshotKey,
  ) -> Self {
    let opacity = view_opacity(element);
    let filter = view_filter(element);
    let quad_paint = (opacity < 1.0 || filter.is_some()).then(|| effect_paint(1.0, opacity, filter));
    // The content occupies the top-left canvas*scale pixels of the
    // (ceil-padded) texture, the key's canvas; mapping exactly that region
    // onto the logical-size quad keeps the composite pixel-exact under the
    // root scale transform where the two boxes are one. The quad is this
    // frame's box (`painted`), not the key's: the key keeps the box its
    // texture was made for while the raster is stretched or a track runs,
    // and that box's far edge can be most of a device pixel away from
    // where this frame's box ends, so boundaries that touch would show a
    // seam or an overlap. At this frame's box each edge meets its
    // neighbour's, and the texture is stretched by under one more device
    // pixel. The quad sits at the detached paint offset the recording
    // countered, pushed out by the shader outset (the effect's transparent
    // margin extends past the box symmetrically), stretched like the box,
    // so the content box inside the canvas lands exactly on this frame's
    // box. The region never reaches past the texture: a canvas a float
    // noise above whole texels samples exactly those texels.
    let canvas = key.canvas();
    let (tex_w, tex_h) = key.texture_dims();
    let src = Rect::new(
      Point::zero(),
      Size::new((canvas.width * key.scale).min(tex_w as f32), (canvas.height * key.scale).min(tex_h as f32)),
    );
    let over = |now: f32, made: f32| if made > 0.0 { now / made } else { 1.0 };
    let stretch = (over(painted.width, key.width), over(painted.height, key.height));
    let margin = (key.outset * stretch.0, key.outset * stretch.1);
    let dst = Rect::new(
      Point::new(offset.0 - margin.0, offset.1 - margin.1),
      Size::new(painted.width + 2.0 * margin.0, painted.height + 2.0 * margin.1),
    );
    Self { element, own, frame, src, dst, quad_paint, opacity, filter, scale: key.scale, stretch }
  }

  // The shared prologue/epilogue of every composite leg: the hoisted matrix,
  // then the backdrop layer in box space (bounds before any scroll, like
  // record_node's emission order), then the content.
  fn draw(&self, builder: &mut DisplayListBuilder, content: impl FnOnce(&mut DisplayListBuilder)) {
    if let Some(m) = &self.own {
      builder.save();
      builder.transform(m);
    }
    emit_backdrop(builder, self.element, self.frame, 1.0);
    content(builder);
    if self.own.is_some() {
      builder.restore();
    }
  }

  /// Composite a rasterization (the raw snapshot or a shader pass output) as
  /// the boundary's quad.
  fn draw_texture(&self, builder: &mut DisplayListBuilder, texture: &Texture) {
    self.draw(builder, |b| {
      b.draw_texture_rect(texture, &self.src, &self.dst, TextureSampling::Linear, self.quad_paint.as_ref());
    });
  }

  /// Rasterization failed: replay the recording inline this frame. The
  /// recording carries its own device-scale transform and content offset, so
  /// counter both before replaying; it is laid out at the key's box, so it
  /// is stretched onto this frame's box like the quad.
  fn draw_inline(&self, builder: &mut DisplayListBuilder, dl: &DisplayList) {
    self.draw(builder, |b| {
      b.save();
      b.translate(self.dst.origin.x, self.dst.origin.y);
      b.scale(self.stretch.0 / self.scale, self.stretch.1 / self.scale);
      draw_dl_with_effects(b, dl, self.opacity, self.filter);
      b.restore();
    });
  }
}

// A node's painted box relative to its parent-translated origin: a laid-out
// node's box as the walk snapped it (`frame`, its ctx.size). A detached
// (d-*) node has none, but it is still drawn into a definite rectangle: its
// kind's painted box, sized with the same inherited frame its build()
// reads, so snapshot, capture and paint box the node identically by
// construction rather than by separate derivations. The returned offset is
// the node's own paint offset, countered in the recording so the content
// lands at the texture origin and restored on the composited quad's dst -
// except for a View, whose offset (translate) lives in the matrix that
// Hoist::Transform keeps out of the recording anyway.
pub(super) fn painted_box(element: &Element, frame: Size) -> (f32, f32, (f32, f32)) {
  match element.painted_size() {
    Some(_) => (frame.width, frame.height, (0.0, 0.0)),
    None => {
      let local = element.kind.local_bounds(frame);
      let offset = match &element.kind {
        ElementKind::View(_) => (0.0, 0.0),
        _ => (local.origin.x, local.origin.y),
      };
      (local.size.width, local.size.height, offset)
    }
  }
}

// Replays a recorded display list under the view's composite-time effects: a
// filter needs a save_layer carrying it (draw_display_list has only an
// opacity argument), plain opacity stays on the cheap path.
fn draw_dl_with_effects(
  builder: &mut DisplayListBuilder,
  dl: &DisplayList,
  opacity: f32,
  filter: Option<&FilterState>,
) {
  if filter.is_some() {
    let paint = effect_paint(0.0, opacity, filter);
    let bounds = Rect::new(Point::new(-CLIP_INF, -CLIP_INF), Size::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
    crate::rendertree::counters::note_save_layer();
    builder.save_layer(&bounds, Some(&paint));
    crate::rendertree::counters::note_draw();
    builder.draw_display_list(dl, 1.0);
    builder.restore();
  } else {
    crate::rendertree::counters::note_draw();
    builder.draw_display_list(dl, opacity);
  }
}

// Composites a Recording boundary's cached content. A View boundary's cache
// holds children only (Hoist::Full): its current matrix and scroll, snapped
// by the caller (`hoisted`), and its clip are applied around the draw here,
// so transform and scroll writes replay the same cache. A non-View
// boundary's cache holds everything and draws bare. `size` is the node's
// painted box.
pub(super) fn draw_cached_recording(
  builder: &mut DisplayListBuilder,
  element: &Element,
  hoisted: Option<(Matrix, Option<Vector>)>,
  dl: &DisplayList,
  size: Size,
) {
  let opacity = view_opacity(element);
  let filter = view_filter(element);
  if let Some((m, scroll)) = hoisted {
    builder.save();
    builder.transform(&m);
    apply_clip(builder, element, size);
    // Box-space bounds: before the scroll translate, like record_node's
    // emission order.
    emit_backdrop(builder, element, size, 1.0);
    apply_scroll(builder, scroll);
    draw_dl_with_effects(builder, dl, opacity, filter);
    builder.restore();
  } else {
    draw_dl_with_effects(builder, dl, opacity, filter);
  }
}

// Snapshot gate: the subtree is rasterized into a texture at the density
// its quad composites at (the grid inside the node's own matrix, at rest
// or at a running transition's end: text::raster_density, so a zoom
// stretches the texture and re-rasterizes once) and composited as a single
// quad until
// something inside it changes, its layout size changes, or that density
// changes. Content
// painting outside the layout box is cropped (unlike a recording boundary);
// the crop happens in untransformed local space, since the boundary's own
// transform is hoisted out of the raster and applied to the quad instead.
// All storage is exact-size. A declared boundary shader adds one pass over
// the rasterization and composites its output instead (see View::set_shader).
pub(super) fn snapshot_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  aa: bool,
) {
  // The texture outlives this frame's viewport (an ancestor scroll does not
  // invalidate it), so the raster must hold the whole subtree.
  let cull = ctx.cull.take();
  snapshot_node_unculled(scene, node_id, ctx, builder, aa);
  ctx.cull = cull;
}

fn snapshot_node_unculled<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  aa: bool,
) {
  let element = scene.node(node_id);
  // The inherited frame, copied out for the BoundaryComposite's backdrop
  // emission (ctx cannot be re-borrowed at draw time).
  let frame = ctx.size;
  // This frame's box, where the quad draws: the snapped box on a rounded
  // grid, the layout box where a running track or `pixelSnap={false}` has
  // the rounding off.
  let (painted_w, painted_h, offset) = painted_box(element, ctx.size);
  let painted = Size::new(painted_w, painted_h);
  let layout = element.painted_size().unwrap_or(painted);
  let pixel_snap_off = ctx.pixel_snap_off;
  let own = snapped_own(element, ctx.size, &ctx.grid);
  // The raster density: the grid inside the node's own matrix (what the
  // quad is scaled by on composite), followed at rest and ahead of a known
  // zoom's end like a text layer (text::raster_density), so the texture is
  // pixel-exact where the node rests; the display scale alone, as before,
  // only where there is no grid (a rotation above).
  let display_scale = ctx.platform.display_scale();
  let inner_grid = match &own {
    Some(m) => grid::through(&ctx.grid, m),
    None => ctx.grid,
  };
  let composite = grid::scale(&inner_grid).unwrap_or(display_scale);
  let seen = text::scale_observed(element.scale_seen.get(), composite, ctx.frame);
  element.scale_seen.set(Some(seen));
  let at_rest = seen.at_rest() || ctx.scale_at_rest;
  let factor = ctx.scale_target * scene.scale_transition_factor(node_id);
  let have = match &*element.paint_cache.borrow() {
    Some(PaintCache::Snapshot(snap)) => Some(snap.scale),
    _ => None,
  };
  let (scale, wait) = text::raster_density(have, composite, factor, at_rest);
  if wait {
    scene.note_scale_wait(node_id);
    ctx.platform.request_frame();
  }
  // The key's box, what the texture holds and its raster is laid out at:
  // like the density, a snapped box is content made for one scale (whole
  // device pixels of the grid it was snapped on, a new logical size at
  // every scale). A cache whose layout size and density are this one keeps
  // its box while the rounding is off (a running track, `pixelSnap={false}`),
  // while its raster is composited stretched (`wait`: a scale that moves,
  // written every frame or not), and while this frame's box takes the same
  // texels at that density (a scale that drifts by less than a held step,
  // the end of an ease, the frame a track settles on): a snapshot is reused
  // as it is under the motion, and when a track starts and ends, and the
  // last bits of a box divided by a grid scale never re-make it. Otherwise
  // the key takes this frame's box: the snapped box on a rounded grid, the
  // layout box under `pixelSnap={false}` (mapped 1:1, fractional, as that
  // prop asks), and under a running track, for a laid-out node, the layout
  // box in whole texels of the density (whole_texels), so the content laid
  // out at it covers the canvas to its last texel and the stretch onto the
  // layout box shows no partly covered edge. A raster for a new density
  // (past the motion bound, at rest, a known zoom's end) is new storage
  // anyway; one made under a track re-rasterizes once at rest if its box
  // takes other texels there. A `pixelSnap` change above it makes the
  // interior stale.
  //
  // Inside the raster the snapshot owns the look at rest of what it holds
  // (BuildContext::in_snapshot_raster): a raster that holds content made
  // mid-motion for rest to re-make (an unhinted text, a recording waiting
  // for its look), at a density still kept, is made stale once the node is
  // at rest, so it re-renders in place once, for rest, and the texts come
  // out hinted; until then it asks for the looks rest takes, even where its
  // density does not wait (a scale that stopped on the raster's own).
  let laid_out = element.painted_size().is_some();
  let mut kept = None;
  let mut holds_pending = false;
  if let Some(PaintCache::Snapshot(snap)) = &mut *element.paint_cache.borrow_mut() {
    if snap.pixel_snap_off != ctx.pixel_snap_off {
      snap.valid = false;
    }
    if snap.valid && snap.rest_pending && snap.scale == scale {
      holds_pending = true;
      if at_rest {
        snap.valid = false;
      }
    }
    if snap.layout == layout && snap.scale == scale {
      let texels_of = |w: f32, h: f32| SnapshotKey { width: w, height: h, scale, outset: 0.0 }.texture_dims();
      let same_texels = texels_of(painted_w, painted_h) == texels_of(snap.width, snap.height);
      if ctx.grid.unrounded() || wait || same_texels {
        kept = Some((snap.width, snap.height));
      }
    }
  }
  if holds_pending && !at_rest && !wait {
    scene.note_scale_wait(node_id);
    ctx.platform.request_frame();
  }
  let tracked = ctx.grid.unrounded() && !ctx.pixel_snap_off;
  let (width, height) = match kept {
    Some(cached) => cached,
    None if laid_out && tracked => (whole_texels(painted_w, scale), whole_texels(painted_h, scale)),
    None => (painted_w, painted_h),
  };
  // The frame the raster is laid out in: the key's box for a laid-out
  // node, so the texture holds the box the key names even when that is
  // not this frame's (the cached box, re-rendered in place while
  // stretched). A detached node's box comes from the frame it inherits,
  // which stays.
  let raster_frame = if laid_out { Size::new(width, height) } else { frame };
  // A raster made for a scale the node rests at (at rest, or a known zoom's
  // end) is told to its record walk below: the texts inside raster hinted
  // for exactly this density, as any text at rest, and wait for nothing.
  let for_rest = at_rest || (factor != 1.0 && text::scale_held(scale, composite * factor));
  let box_key = SnapshotKey { width, height, scale, outset: 0.0 };
  let (tex_w, tex_h) = box_key.texture_dims();

  // Without a positive painted box there is nothing to rasterize into; paint
  // inline so overflowing content still shows up.
  if tex_w == 0 || tex_h == 0 {
    record_node(scene, node_id, ctx, builder, Hoist::None);
    return;
  }

  let hoist = match own {
    Some(m) => Hoist::Transform(m),
    None => Hoist::None,
  };
  // The raster is a raster root (record_raster): inside it the grid is the
  // texture's, the content countered by the paint offset (and pushed out by
  // a shader outset below).
  let paint_offset = Vector::new(offset.0, offset.1);

  // The boundary shader (views only) and its pending-write flag, consumed
  // here whichever branch runs: every shaded branch re-runs the pass, and
  // the plain path has nothing to re-run.
  let (shader, shader_dirty) = match &element.kind {
    ElementKind::View(v) => (v.shader.as_ref(), v.take_shader_dirty()),
    _ => (None, false),
  };

  // A withdrawn shader keeps the snapshot: the source texture and its
  // validity are untouched, only the pass output (and any history) drops.
  // Except with an outset - that canvas is bigger than the plain box-sized
  // texture, so the storage cannot be kept.
  if shader.is_none() {
    let mut cache = element.paint_cache.borrow_mut();
    let drop_all = if let Some(PaintCache::Snapshot(snap)) = &mut *cache {
      snap.shaded.take().is_some_and(|sc| sc.outset > 0.0)
    } else {
      false
    };
    if drop_all {
      cache.take();
    }
  }

  if let Some(decl) = shader {
    // The outset grows the canvas symmetrically: content sits at
    // (outset, outset) clipped to the layout box, the margin is transparent
    // and belongs to the effect, and the composited quad extends past the
    // box by the same amount. The pass and all textures work at canvas
    // size, which the key carries; the quad is this frame's box grown by
    // the key's outset.
    // The outset is whole device pixels (rounded up: the margin is at
    // least the declared one), so the content inside the canvas and the
    // quad it composites as both sit on the grid.
    let outset = (decl.outset.max(0.0) * scale).ceil() / scale;
    let key = SnapshotKey { outset, ..box_key };
    let (tex_w, tex_h) = key.texture_dims();
    let quad = BoundaryComposite::new(element, own, frame, offset, painted, &key);

    // Valid content with matching shader storage: composite the cached
    // output, re-running the pass in place first when a declaration write
    // is pending (the params path - the snapshot is not re-rasterized). An
    // outset change or a `previous` toggle fails the compare instead; both
    // change what storage must exist.
    let cached = {
      let cache = element.paint_cache.borrow();
      match &*cache {
        Some(PaintCache::Snapshot(snap)) => match &snap.shaded {
          Some(sc) if snap.valid && key.matches(snap) && sc.history.is_some() == decl.previous => {
            Some((snap.texture.clone(), sc.output.clone(), sc.history.clone(), snap.rest_pending))
          }
          _ => None,
        },
        _ => None,
      }
    };
    if let Some((source, output, history, pending)) = cached {
      service_captures_under_cache(scene, node_id, ctx, hoist);
      ctx.rest_pending += u32::from(pending);
      if shader_dirty {
        if let Err(e) = ctx.alloy.rerun_node_shader(decl, &source, &output, history.as_ref(), tex_w, tex_h) {
          log::warn!("boundary shader re-run failed for node {node_id}: {e}");
        }
      }
      ctx.snapshots_reused += 1;
      quad.draw_texture(builder, &output);
      return;
    }

    // Content changed (or the declaration needs different storage): record,
    // rasterize and run the pass in one trip. Dimension-matched storage is
    // re-rendered in place; exact storage means only an exact key match
    // qualifies.
    let mut sub = DisplayListBuilder::new(None);
    sub.scale(scale, scale);
    if outset > 0.0 {
      sub.translate(outset, outset);
      // Without an outset the box crop is the texture viewport itself; with
      // a margin the crop must be explicit, or overflowing content would
      // paint into the effect's transparent margin.
      sub.clip_rect(&Rect::new(Point::new(0.0, 0.0), Size::new(width, height)), ClipOperation::Intersect);
    }
    let raster_grid = grid::raster(scale, Vector::new(outset, outset));
    let rest_pending = record_raster(scene, node_id, ctx, &mut sub, hoist, raster_grid, raster_frame, for_rest);
    let Some(dl) = sub.build() else { return };

    // Reusable storage: the source (plain or shaded), plus output and
    // history when the cache was already shaded. A plain cache's texture
    // counts as outset 0, so declaring a no-outset shader over an existing
    // snapshot re-renders its storage instead of reallocating.
    let retained = {
      let cache = element.paint_cache.borrow();
      match &*cache {
        Some(PaintCache::Snapshot(snap)) if key.matches(snap) => {
          let output = snap.shaded.as_ref().map(|sc| sc.output.clone());
          let history = snap.shaded.as_ref().and_then(|sc| sc.history.clone());
          Some((snap.texture.clone(), output, history))
        }
        _ => None,
      }
    };
    // Storage roles for this rasterization. Without `previous` the source
    // re-renders in place. With it, the roles rotate: render into the old
    // history's storage (fresh when none) and bind the old source as
    // uPrevious - the previous rasterization by construction, no copy.
    let (render_into, history_pass, reuse_output) = match &retained {
      Some((source, output, history)) => {
        if decl.previous {
          (history.clone(), Some(source.clone()), output.clone())
        } else {
          (Some(source.clone()), None, output.clone())
        }
      }
      None => (None, None, None),
    };
    let result = ctx.alloy.rasterize_shaded(
      &dl,
      tex_w,
      tex_h,
      aa,
      decl,
      render_into.as_ref(),
      reuse_output.as_ref(),
      history_pass.as_ref(),
    );
    match result {
      Ok((source, output, history)) => {
        if retained.is_some() {
          ctx.snapshots_rerendered += 1;
        } else {
          ctx.snapshots_rasterized += 1;
        }
        publish_snapshot(element, ctx, &source, tex_w, tex_h);
        quad.draw_texture(builder, &output);
        *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot(SnapshotCache {
          texture: source,
          width,
          height,
          scale,
          valid: true,
          layout,
          pixel_snap_off,
          rest_pending,
          shaded: Some(ShadedCache { output, outset, history }),
        }));
      }
      Err(e) => {
        // Paint inline (unshaded) this frame and drop the cache, so no
        // stale storage is offered for in-place reuse on the next damage.
        log::warn!("shaded snapshot failed for node {node_id}: {e}; painting inline unshaded");
        element.paint_cache.borrow_mut().take();
        quad.draw_inline(builder, &dl);
      }
    }
    return;
  }

  let quad = BoundaryComposite::new(element, own, frame, offset, painted, &box_key);

  let reusable = {
    let cache = element.paint_cache.borrow();
    match &*cache {
      Some(PaintCache::Snapshot(snap)) if snap.valid && box_key.matches(snap) => {
        Some((snap.texture.clone(), snap.rest_pending))
      }
      _ => None,
    }
  };
  if let Some((texture, pending)) = reusable {
    service_captures_under_cache(scene, node_id, ctx, hoist);
    // A raster holding content for rest to re-make carries it into an
    // enclosing snapshot's or recording's count, which must be re-made at
    // rest for it too.
    ctx.rest_pending += u32::from(pending);
    ctx.snapshots_reused += 1;
    quad.draw_texture(builder, &texture);
    return;
  }

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  if offset != (0.0, 0.0) {
    sub.translate(-offset.0, -offset.1);
  }
  let raster_grid = grid::raster(scale, -paint_offset);
  let rest_pending = record_raster(scene, node_id, ctx, &mut sub, hoist, raster_grid, raster_frame, for_rest);
  let Some(dl) = sub.build() else { return };

  // Stale storage at unchanged dimensions is re-rendered in place: the
  // offscreen draw clears and rewrites the full allocation, so no stale
  // pixels survive. Exact-size storage means any dimension change
  // reallocates.
  let retained = {
    let cache = element.paint_cache.borrow();
    match &*cache {
      Some(PaintCache::Snapshot(snap)) if box_key.matches(snap) => Some(snap.texture.clone()),
      _ => None,
    }
  };
  if let Some(texture) = retained {
    match ctx.alloy.render_display_list_into_texture(&dl, &texture, tex_w, tex_h, aa) {
      Ok(()) => {
        ctx.snapshots_rerendered += 1;
        publish_snapshot(element, ctx, &texture, tex_w, tex_h);
        quad.draw_texture(builder, &texture);
        *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot(SnapshotCache {
          texture,
          width,
          height,
          scale,
          valid: true,
          layout,
          pixel_snap_off,
          rest_pending,
          shaded: None,
        }));
        return;
      }
      Err(e) => {
        log::warn!("snapshot re-render failed for node {node_id}: {e}; reallocating");
        element.paint_cache.borrow_mut().take();
      }
    }
  }

  match ctx.alloy.render_display_list_to_texture(&dl, tex_w, tex_h, aa) {
    Ok(texture) => {
      ctx.snapshots_rasterized += 1;
      publish_snapshot(element, ctx, &texture, tex_w, tex_h);
      quad.draw_texture(builder, &texture);
      *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot(SnapshotCache {
        texture,
        width,
        height,
        scale,
        valid: true,
        layout,
        pixel_snap_off,
        rest_pending,
        shaded: None,
      }));
    }
    Err(e) => {
      log::warn!("snapshot rasterization failed for node {node_id}: {e}; painting inline");
      quad.draw_inline(builder, &dl);
    }
  }
}

// Record a snapshot boundary's subtree as a raster root: inside it the grid
// is the texture's (`grid`), rounding off only under `pixelSnap={false}` -
// a running track's motion ends at a raster, so the contents stay on their
// texture's grid. The node is laid out in `frame`, the box the texture
// holds (snapshot_node_unculled). No scale transition outside reaches the
// texts inside either: the raster's density already holds where the chain
// is headed (text::raster_density), so they raster for exactly that
// density, and `for_rest` tells them it is one the node rests at. The walk
// is inside a snapshot raster (BuildContext::in_snapshot_raster): the
// texts and recordings wait for nothing of their own. Returns whether the
// raster holds content made mid-motion that the snapshot re-renders for
// once it is at rest (BuildContext::rest_pending).
#[allow(clippy::too_many_arguments)]
fn record_raster<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  sub: &mut DisplayListBuilder,
  hoist: Hoist,
  grid: grid::GridMap,
  frame: Size,
  for_rest: bool,
) -> bool {
  let slot_grid = ctx.grid;
  let slot_frame = ctx.size;
  let outer_rest = ctx.scale_at_rest;
  let outer_target = ctx.scale_target;
  let outer_raster = ctx.in_snapshot_raster;
  let pending_before = ctx.rest_pending;
  ctx.grid = grid.or_unrounded(ctx.pixel_snap_off);
  ctx.size = frame;
  ctx.scale_at_rest = outer_rest || for_rest;
  ctx.scale_target = 1.0;
  ctx.in_snapshot_raster = true;
  ctx.raster_root = true;
  record_node(scene, node_id, ctx, sub, hoist);
  ctx.grid = slot_grid;
  ctx.size = slot_frame;
  ctx.scale_at_rest = outer_rest;
  ctx.scale_target = outer_target;
  ctx.in_snapshot_raster = outer_raster;
  ctx.rest_pending > pending_before
}

/// The texels a canvas side of `side` logical px takes at `scale`: rounded
/// up, where a side within the tolerance above a whole count is that count
/// (TEXEL_CEIL_EPSILON, or TEXEL_CEIL_RELATIVE of the texels where that is
/// more). Never below zero.
pub(crate) fn canvas_texels(side: f32, scale: f32) -> u32 {
  let exact = side * scale;
  let tolerance = TEXEL_CEIL_EPSILON.max(exact.abs() * TEXEL_CEIL_RELATIVE);
  (exact - tolerance).ceil().max(0.0) as u32
}

// A side of a raster box made under a running track, as the key holds it:
// `side` in whole texels of the density `scale`, at least
// RASTER_MIN_TEXELS, so the content laid out at it covers the canvas to
// its last texel (the float noise of texels / scale adds none:
// SnapshotKey::texture_dims). A side that is not positive, or a scale that
// is no scale, stays as it is.
pub(crate) fn whole_texels(side: f32, scale: f32) -> f32 {
  if side <= 0.0 || !(scale > 0.0 && scale.is_finite()) {
    return side;
  }
  (side * scale).round().max(RASTER_MIN_TEXELS) / scale
}

// Re-point a boundary's vended texture id (see RenderTree::snapshot_texture)
// at the rasterization just produced. A boundary nobody asked for publishes
// nothing.
fn publish_snapshot(element: &Element, ctx: &BuildContext<'_>, texture: &Texture, tex_w: u32, tex_h: u32) {
  if let Some(id) = element.snapshot_texture_id.get() {
    ctx.alloy.publish_snapshot_texture(id, texture, tex_w, tex_h);
  }
}
