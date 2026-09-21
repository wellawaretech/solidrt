use crate::impellers::{ClipOperation, Color, DisplayListBuilder, Matrix, Paint, Point, Rect, RoundingRadii, Size};
use taffy::style::Overflow;
use taffy::{AvailableSpace, NodeId};

use crate::rendertree::boundary::{self, Hoist};
use crate::rendertree::cull::{self, CullRect};
use crate::rendertree::{
  BackdropPass, BoundaryMode, BuildContext, Element, ElementKind, FilterState, FrameDamage, LayoutContext, PaintCache,
  PlatformContext, RenderTree,
};
use crate::{CaptureDone, CaptureInfo};

// Large finite extent used to leave one axis effectively unclipped when only the
// other axis has non-visible overflow. clip_rect requires a finite rectangle.
pub(super) const CLIP_INF: f32 = 1.0e7;

// Runs taffy layout. Safe to call repeatedly: taffy's per-node cache makes
// a second call cheap when nothing has been invalidated since the previous run.
pub fn layout_phase(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) {
  let Some(root_id) = tree.root else { return };
  let (width, height) = platform.window_size();

  if platform.take_window_size_dirty() {
    tree.invalidate_cache(root_id);
    // A resized window relayouts nearly everything; the frame is fully
    // damaged by definition.
    tree.damage_all();
  }

  let available_space =
    taffy::Size { width: AvailableSpace::Definite(width), height: AvailableSpace::Definite(height) };
  let mut layout_ctx = LayoutContext { render_tree: tree, platform, alloy, hidden_depth: 0 };
  taffy::compute_root_layout(&mut layout_ctx, NodeId::from(root_id), available_space);
  // Declaring nodes the pass moved slide from where they were painted.
  tree.start_layout_slides();
}

/// Repaint-boundary counts for one painted frame: subtrees drawn from their
/// retained recording vs freshly recorded, and snapshot boundaries drawn from
/// their retained texture, re-rendered into retained storage, or freshly
/// rasterized into a new allocation.
#[derive(Clone, Copy, Default)]
pub struct PaintStats {
  /// Nodes the walk entered (viewport-culled subtrees excluded, see cull.rs).
  pub nodes_painted: u32,
  pub boundaries_reused: u32,
  pub boundaries_recorded: u32,
  pub snapshots_reused: u32,
  pub snapshots_rerendered: u32,
  pub snapshots_rasterized: u32,
  /// Glass panels re-filtered ahead of a fading ancestor's opacity group
  /// (emit_backdrops_below); nonzero only while such a fade runs.
  pub backdrops_prepainted: u32,
  /// The frame's resolved damage area in logical px^2 (window area on a
  /// fully damaged frame); see RenderTree::frame_damage.
  pub damage_px: f32,
}

// Picks up any cache invalidations queued by onLayout handlers, then paints.
pub fn paint_phase(
  builder: &mut DisplayListBuilder,
  tree: &mut RenderTree,
  platform: &PlatformContext,
  alloy: &crate::Context,
) -> PaintStats {
  let Some(root_id) = tree.root else { return PaintStats::default() };
  let (width, height) = platform.window_size();

  layout_phase(tree, platform, alloy);

  // Partial repaint: the damaged ids' last_extent cells still hold their
  // extents as of the LAST walk - the old half of the damage union (where
  // pixels must be erased). The walk below rewrites the cells; the second
  // pass after it reads the new half. Taken after layout_phase so the ids
  // noted by set_unrounded_layout (relayout-shifted nodes) are included.
  let (damaged, damage_full) = tree.damage_ledger().take();
  let size = Size::new(width, height);
  let window_rect = Rect::new(Point::zero(), size);
  tree.node(root_id).last_extent.set(cull::Extent::Bounded(window_rect));

  // The walk borrows the tree for the BuildContext's lifetime, so it is
  // scoped: only plain stats leave the block.
  let (mut stats, damage, regions) = {
    let mut ctx = BuildContext::new(platform, alloy);
    ctx.size = size;
    // The root's boxes come from its layout like every other node's (the hit
    // side derives them per element, so a padded root must agree here too);
    // before the first layout the window is the frame.
    ctx.content =
      tree.node(root_id).layout.as_ref().map(|l| l.content_box()).unwrap_or(Rect::new(Point::zero(), ctx.size));
    // Nothing outside the window is visible: the root cull rect is the window.
    ctx.cull = Some(window_rect);
    ctx.to_window = Some(euclid::default::Transform2D::identity());

    let mut damage = cull::Extent::Empty;
    if !damage_full {
      for &id in &damaged {
        damage = damage.union(damaged_extent(tree, id));
      }
    }

    build_recursive(tree, root_id, &mut ctx, builder);

    if !damage_full {
      for &id in &damaged {
        damage = damage.union(damaged_extent(tree, id));
      }
    }
    let stats = PaintStats {
      nodes_painted: ctx.nodes_painted,
      boundaries_reused: ctx.boundaries_reused,
      boundaries_recorded: ctx.boundaries_recorded,
      snapshots_reused: ctx.snapshots_reused,
      snapshots_rerendered: ctx.snapshots_rerendered,
      snapshots_rasterized: ctx.snapshots_rasterized,
      backdrops_prepainted: ctx.backdrops_prepainted,
      damage_px: 0.0,
    };
    let regions = std::mem::take(&mut ctx.backdrop_regions);
    (stats, damage, regions)
  };
  let frame_damage = tree.damage_ledger().resolve_walk(damage, damage_full, regions, window_rect);
  stats.damage_px = frame_damage.area(size);
  // Any capture request whose node the walk never visited targets a node that
  // is not in the live tree; fail it rather than leave its promise pending.
  alloy.fail_unserviced_captures();
  // Deliver every capture outcome now the walk is done, so callbacks (which may
  // read back or free textures) run out of the tree borrow.
  alloy.deliver_captures();
  release_retired_textures(tree, alloy);
  stats
}

/// Apply GPU content writes since the last frame (target re-renders, uploads,
/// camera frames) to the tree: they change pixels behind unchanged texture ids
/// and leave no tree damage of their own, and a baked snapshot boundary over
/// one would keep replaying stale pixels. Every frame producer calls this
/// before resolving its frame; returns whether anything changed.
pub(crate) fn apply_content_changes(tree: &mut RenderTree, alloy: &crate::Context) -> bool {
  let content = alloy.take_content_changes();
  let changed = !content.is_empty();
  if changed {
    tree.texture_content_changed(&content);
  }
  changed
}

/// End-of-frame texture housekeeping, run by every frame producer once its
/// display list is settled. Vended snapshot textures whose boundary was
/// deleted join the deferred destroys, so a consumer still sampling one keeps
/// its last pixels. Then deferred destroys reclaim ids the live tree no
/// longer references - the frame's display list is already recorded (Rc'd
/// Impeller handles keep its textures alive), and any still-referenced id
/// stays queued so a build never finds a hole in the registry. Gated so the
/// tree scan only runs when a destroy is actually pending.
pub(crate) fn release_retired_textures(tree: &mut RenderTree, alloy: &crate::Context) {
  for id in tree.take_released_snapshot_textures() {
    alloy.release_borrowed(id);
  }
  if alloy.has_pending_destroys() {
    alloy.reclaim_destroyed(&tree.referenced_texture_ids());
  }
}

/// Resolve the accumulated damage WITHOUT a paint walk, for the present-only
/// reuse path (PendingFrame::commit): the tree is unchanged since the last
/// walk, so a damaged id's cell is both its old and its new extent. Only
/// GPU-content damage lands here - texture nodes whose pixels changed behind
/// an unchanged display list (texture_content_changed notes them without a
/// revision bump).
pub fn resolve_reuse_damage(tree: &mut RenderTree, window: Size) -> FrameDamage {
  let (damaged, full) = tree.damage_ledger().take();
  let window_rect = Rect::new(Point::zero(), window);
  let mut damage = cull::Extent::Empty;
  if !full {
    for &id in &damaged {
      damage = damage.union(damaged_extent(tree, id));
    }
  }
  tree.damage_ledger().resolve_reuse(damage, full, window_rect)
}

// A damaged node's window extent from its cell. A node the walk has never
// considered (a span, drawn by its text; a node born under a culled parent)
// has an Empty cell and borrows the nearest ancestor's - conservative, and
// the ascent ends at the root, whose cell is the window. A destroyed id
// contributes nothing: its removal already noted the parent.
fn damaged_extent(tree: &RenderTree, id: u64) -> cull::Extent {
  let mut current = Some(id);
  while let Some(cur) = current {
    let Some(element) = tree.try_node(cur) else { return cull::Extent::Empty };
    match element.last_extent.get() {
      cull::Extent::Empty => current = element.parent,
      extent => return extent,
    }
  }
  cull::Extent::Empty
}

/// Lay out and paint the whole tree into a fresh display list and submit it to
/// the render thread: the minimal "draw the tree once" path. Unlike the runner's
/// frame loop it carries no demand gating, retained-list reuse, post-layout hook
/// or stats overlay (all of which are policy); it is what a flux + alloy app
/// calls each frame to put the current tree on screen. Returns the frame's paint
/// stats.
pub fn render(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> PaintStats {
  // The runner's frame loop drains content changes itself, ahead of its
  // display-list reuse check; this path is the frame producer for everything
  // else, so it must apply them too.
  apply_content_changes(tree, alloy);

  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, alloy);
  if let Some(dl) = builder.build() {
    let damage = crate::PresentDamage::from_frame(tree.frame_damage(), platform.display_scale());
    if alloy.submit(dl, damage).is_err() {
      log::warn!("rendertree::render: render thread unavailable, dropping frame");
    }
  }
  stats
}

// A View's own box transform: the user chain only, no design-size fit (the fit
// maps children into the box, it never moves the box itself, and it is
// content - recorded by record_node so caches and captures hold fitted,
// box-sized output). Resolved against the view's border box - the same frame
// the hit side passes (okf/done/padding-box-divergence.md); detached views
// fall back to the inherited frame. record_node applies it under Hoist::None; boundary
// callers hoist it out of the cached content and apply it at composite time,
// so the cache stays reusable under transform-only changes and Snapshot mode
// never bakes a rotation/scale into the layout-box crop. None for non-View
// kinds, which paint transform-free content in build().
pub(super) fn own_matrix(element: &Element, inherited: Size) -> Option<Matrix> {
  match &element.kind {
    ElementKind::View(v) => {
      let box_size = element.frame_size(inherited);
      Some(v.box_matrix(box_size))
    }
    _ => None,
  }
}

// Per-axis overflow clipping from the element's layout style; no layout means
// no clip.
fn overflow_clips(element: &Element) -> (bool, bool) {
  element
    .layout
    .as_ref()
    .map(|l| (l.style.overflow.x != Overflow::Visible, l.style.overflow.y != Overflow::Visible))
    .unwrap_or((false, false))
}

// Emits the element's overflow clip (rounded when a View clips both axes) in
// node-local, pre-scroll BOX space: the rect is the layout box, so it must be
// applied under the user chain but before any design-size fit (a box-sized rect
// emitted in design space clips the wrong rectangle in both fit directions -
// okf/backlog/overflow-viewbox-clip.md). Shared by record_node and the
// Recording boundary composite path so the two cannot diverge. No-op without
// a clip.
pub(super) fn apply_clip(builder: &mut DisplayListBuilder, element: &Element) {
  let (clip_x, clip_y) = overflow_clips(element);
  if !clip_x && !clip_y {
    return;
  }
  let layout = element.layout.as_ref().expect("overflow clip requires layout");
  let size = layout.size();
  let (w, h) = (size.width, size.height);
  // Rounded clip only applies when the whole box is clipped (both axes);
  // a single-axis clip has no meaningful corners to round.
  let clip_radius = match &element.kind {
    ElementKind::View(v) if clip_x && clip_y => v.clip_radius,
    _ => None,
  };
  if let Some([tl, tr, br, bl]) = clip_radius {
    let rect = Rect::new(Point::new(0.0, 0.0), Size::new(w, h));
    let radii = RoundingRadii {
      top_left: Point::new(tl, tl),
      top_right: Point::new(tr, tr),
      bottom_right: Point::new(br, br),
      bottom_left: Point::new(bl, bl),
    };
    builder.clip_rounded_rect(&rect, &radii, ClipOperation::Intersect);
  } else {
    let x_min = if clip_x { 0.0 } else { -CLIP_INF };
    let y_min = if clip_y { 0.0 } else { -CLIP_INF };
    let x_max = if clip_x { w } else { CLIP_INF };
    let y_max = if clip_y { h } else { CLIP_INF };
    let rect = Rect::new(Point::new(x_min, y_min), Size::new(x_max - x_min, y_max - y_min));
    builder.clip_rect(&rect, ClipOperation::Intersect);
  }
}

// Scroll offset, in box pixels: applied after the clip (the clip box stays
// put in viewport space while children slide under it) and before any design size
// fit, so one scroll pixel is one box pixel regardless of fit scale - the hit
// side divides by the fit scale instead (View::content_scroll). Positive
// scroll shifts content leftward/upward. No-op for non-Views and unscrolled
// Views.
pub(super) fn apply_scroll(builder: &mut DisplayListBuilder, element: &Element) {
  if let ElementKind::View(view) = &element.kind {
    if let Some(s) = view.scroll {
      builder.translate(-s.x, -s.y);
    }
  }
}

// A View's group opacity; non-Views are always opaque. Like the matrix, it is
// hoisted out of boundary caches and applied at composite time (the opacity
// arg of draw_display_list, or a paint on the snapshot quad), so an opacity
// write replays the same cache.
pub(super) fn view_opacity(element: &Element) -> f32 {
  match &element.kind {
    ElementKind::View(v) => v.opacity.unwrap_or(1.0),
    _ => 1.0,
  }
}

// A View's subtree filter; non-Views (and empty declarations) have none.
// Hoisted like opacity: applied around cached content at composite time, so
// a filter write replays the same cache.
pub(super) fn view_filter(element: &Element) -> Option<&FilterState> {
  match &element.kind {
    ElementKind::View(v) => v.active_filter(),
    _ => None,
  }
}

// The paint carrying a view's composite-time effects: the group opacity in
// the alpha (riding on `rgb` - black for a save_layer, white for a texture
// quad, where white keeps the texture's colors), plus the filter's fused
// color matrix and blur.
pub(super) fn effect_paint(rgb: f32, opacity: f32, filter: Option<&FilterState>) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(rgb, rgb, rgb, opacity));
  if let Some(f) = filter {
    if let Some(cf) = f.to_color_filter() {
      paint.set_color_filter(&cf);
    }
    if let Some(blur) = f.to_image_filter() {
      paint.set_image_filter(&blur);
    }
  }
  paint
}

// A View's backdrop filter, emitted where its content is composited: one
// save_layer whose backdrop argument captures and filters the pixels
// already painted beneath the box, restored immediately so the filtered
// backdrop lands back in the target and the view's own content draws over
// it unfiltered. The blur rides the backdrop argument; color keys ride the
// restore paint's color filter (applied to the captured layer content),
// with an identity matrix filter forcing the capture when only color keys
// are set - Impeller has no color-filter-as-image-filter constructor. The
// element's group opacity rides the restore paint's alpha: the layer is
// emitted outside the opacity group at every composite path (a backdrop
// capture inside a fresh save_layer would read that layer, not the
// window), so the fade is applied here instead - the filtered pixels
// restore at the element's alpha over the unfiltered ones already in the
// target, and glass fades with its panel like CSS composites it. `alpha`
// is the same trick one level up: the opacity of the fading ancestors a
// backdrops-only pass (emit_backdrops_below) is emitting the panel ahead
// of, 1 on the regular paths. The bounds are the layout box in box space,
// so this must be emitted after the view's matrix and before any scroll
// translate. No-op for non-Views, empty declarations, and fully transparent
// elements.
pub(super) fn emit_backdrop(builder: &mut DisplayListBuilder, element: &Element, inherited: Size, alpha: f32) {
  let ElementKind::View(v) = &element.kind else { return };
  let Some(f) = v.active_backdrop_filter() else { return };
  let opacity = v.opacity.unwrap_or(1.0) * alpha;
  if opacity <= 0.0 {
    return;
  }
  let size = element.frame_size(inherited);
  let bounds = Rect::new(Point::zero(), size);
  let backdrop = f.to_backdrop_image_filter();
  let color_filter = f.to_color_filter();
  let paint = (color_filter.is_some() || opacity < 1.0).then(|| {
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, opacity));
    if let Some(cf) = &color_filter {
      paint.set_color_filter(cf);
    }
    paint
  });
  // The filtered region is whatever clip is in effect when the layer is
  // pushed - the layer's `bounds` argument is a content hint, not a clip
  // (unclipped, the capture covers the window; verified live). Clip to the
  // box explicitly, and apply the view's own overflow clip (clipRadius
  // included) here as well: rounded glass is spelled through it, and the
  // snapshot quad path composites with the clip baked into texture pixels
  // only, so the emission cannot rely on a caller having applied it to the
  // live target. Callers that did (record_node, draw_cached_recording)
  // intersect an identical clip, which is a no-op.
  builder.save();
  apply_clip(builder, element);
  builder.clip_rect(&bounds, ClipOperation::Intersect);
  builder.save_layer(&bounds, paint.as_ref(), Some(&backdrop));
  builder.restore();
  builder.restore();
}

// Whether a pending capture targets a node strictly inside `node_id`'s
// subtree. Captures are serviced by the walk reaching their node, so a valid
// boundary cache (whose composite leg skips the subtree) must not stand
// between the walk and a capture - the same exemption the cull skip makes.
// The boundary node's own captures are not the subtree's concern: they were
// already taken at build_recursive entry.
pub(super) fn capture_pending_within(scene: &RenderTree, node_id: u64, alloy: &crate::Context) -> bool {
  if !alloy.has_pending_captures() {
    return false;
  }
  alloy.pending_capture_nodes().iter().any(|&target| {
    let mut current = scene.try_node(target).and_then(|e| e.parent);
    while let Some(id) = current {
      if id == node_id {
        return true;
      }
      current = scene.try_node(id).and_then(|e| e.parent);
    }
    false
  })
}

// The opacity a view fades its subtree by as a group, when it does: below 1
// (a fully transparent group has nothing to show) and not a backdrop root -
// a filtered view's panels see its effect layer by design, and a snapshot's
// see its raster (types.d.ts documents both), so neither is pre-emitted.
fn fading_opacity(element: &Element) -> Option<f32> {
  let opacity = view_opacity(element);
  (opacity > 0.0 && opacity < 1.0 && !cull::is_backdrop_root(element)).then_some(opacity)
}

// The backdrops-only pass over `node_id`'s subtree (see build_recursive):
// the same record walk, under the same matrices, clips, scrolls and fits,
// emitting nothing but the glass panels' backdrop layers at `alpha` (the
// node's opacity, times any fading view's on the way down). The node's own
// panel is left to the regular path that follows. An approximation by
// construction: the group then draws its sharp pixels over blurred ones at
// its opacity (as emit_backdrop does for a panel's own opacity), so at a
// low opacity the glass reads partly sharp; exact group blending would need
// the capture inside the group's layer, which the renderer cannot do. Not
// an isolated_walk: the walk keeps the cull (an offscreen panel is not
// prepainted) and its prepaint count is the frame's; size, content and
// the regions are restored because the regular path follows.
fn emit_backdrops_below<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  alpha: f32,
) {
  let saved_size = ctx.size;
  let saved_content = ctx.content;
  let saved_regions = ctx.backdrop_regions.len();
  ctx.backdrop_pass = Some(BackdropPass { alpha, root: node_id });
  record_node(scene, node_id, ctx, builder, Hoist::None);
  ctx.backdrop_pass = None;
  ctx.size = saved_size;
  ctx.content = saved_content;
  ctx.backdrop_regions.truncate(saved_regions);
}

// A record walk of `node_id` into `sub` that leaves no trace on the frame
// being built: the shared ctx is saved around it - size and content (the
// caller reads them after), the cull (a capture holds the whole subtree,
// not the on-screen part), the backdrop regions the walk pushes (they
// duplicate the frame's own, or push a None under a non-2D transform that
// would degrade the whole resolve to full damage) and the walk counters
// (the frame's stats must not count a discarded or offscreen walk). The
// one place this is done, so a counter added to the ctx cannot be isolated
// in one caller and leak through another.
fn isolated_walk<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  sub: &mut DisplayListBuilder,
  hoist: Hoist,
) {
  let saved_size = ctx.size;
  let saved_content = ctx.content;
  let saved_cull = ctx.cull.take();
  let saved_regions = ctx.backdrop_regions.len();
  let saved_stats = ctx.walk_stats();
  record_node(scene, node_id, ctx, sub, hoist);
  ctx.size = saved_size;
  ctx.content = saved_content;
  ctx.cull = saved_cull;
  ctx.backdrop_regions.truncate(saved_regions);
  ctx.restore_walk_stats(saved_stats);
}

// Reach (and thereby service) the captures inside a boundary whose cache is
// being reused: record the subtree into a discarded builder, purely so the
// walk descends to the capture nodes. The cache itself is untouched - the
// caller still composites it - so a capture never re-rasterizes a snapshot
// or rotates a shader history. No-op unless a pending capture is inside.
pub(super) fn service_captures_under_cache<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  hoist: Hoist,
) {
  if !capture_pending_within(scene, node_id, ctx.alloy) {
    return;
  }
  let mut sub = DisplayListBuilder::new(None);
  isolated_walk(scene, node_id, ctx, &mut sub, hoist);
}

// Repaint-boundary gate: a boundary subtree's paint result is retained (as a
// recording or as rasterized pixels, in node-local coordinates; the parent has
// already translated the builder, and the boundary's own transform is applied
// here at composite time) and replayed from the cache until something inside
// it changes (see RenderTree::invalidate_paint).
fn build_recursive<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
) {
  // A backdrops-only pass records straight through: no captures, no cache
  // reuse (a cached boundary's panels are baked in its recording, and the
  // pass is here precisely because the replay will read the wrong target),
  // no region pushes (the regular walk behind it makes them).
  if ctx.backdrop_pass.is_some() {
    record_node(scene, node_id, ctx, builder, Hoist::None);
    return;
  }

  // On-demand captures (captureSnapshot) are serviced as the walk reaches the
  // node, before its own paint. This does not draw into `builder`; the node
  // still paints normally below. Guarded by a cheap emptiness check so the
  // no-capture common case costs one borrow per node.
  if ctx.alloy.has_pending_captures() {
    let requests = ctx.alloy.take_node_captures(node_id);
    if !requests.is_empty() {
      service_captures(scene, node_id, ctx, requests);
    }
  }

  let element = scene.node(node_id);
  // Note a backdrop panel's window-space region for damage widening,
  // whichever composite path draws it below: a change within the blur's
  // reach of the panel alters the panel's pixels, so damage resolves must
  // be able to pull the whole region into the repaint rect.
  if let ElementKind::View(v) = &element.kind {
    if let Some(f) = v.active_backdrop_filter() {
      let size = element.frame_size(ctx.size);
      let region = cull::Extent::Bounded(Rect::new(Point::zero(), size))
        .transformed(&own_matrix(element, ctx.size).unwrap_or_else(Matrix::identity))
        .to_window(Point::zero(), &ctx.to_window);
      ctx.backdrop_regions.push(match region {
        cull::Extent::Bounded(r) => Some((r, f.blur_outset())),
        _ => None,
      });
    }
  }
  // A view's boundary shader requires repaintBoundary="snapshot": the prop's
  // real cost is snapshot semantics, kept explicit rather than implied. Warn
  // once per declaration write (the dirty flag), not per frame.
  if !matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa) {
    if let ElementKind::View(v) = &element.kind {
      if v.shader.is_some() && v.take_shader_dirty() {
        log::warn!("node {node_id} declares a shader without repaintBoundary=\"snapshot\"; not applied");
      }
    }
  }
  // Glass under a fading group: every composite path below applies this
  // view's opacity as a group - a save_layer, or the opacity argument of a
  // cached replay - and a backdrop capture inside that group reads the
  // group's layer, not the window, so the panel would stay sharp for the
  // whole fade. Emit the panels' backdrop layers ahead of the group, faded
  // by the same opacity, and the group's content then draws over its own
  // blurred backdrop like the element's own opacity is handled in
  // emit_backdrop. Gated on the cached backdrop_below bit, so a fade
  // without glass under it costs nothing.
  if let Some(alpha) = fading_opacity(element) {
    if cull::backdrop_below(scene, node_id) {
      emit_backdrops_below(scene, node_id, ctx, builder, alpha);
    }
  }
  match element.repaint_boundary {
    BoundaryMode::None => record_node(scene, node_id, ctx, builder, Hoist::None),
    BoundaryMode::Recording => {
      let own = own_matrix(element, ctx.size);
      let hoist = if own.is_some() { Hoist::Full } else { Hoist::None };
      let cached = match &*element.paint_cache.borrow() {
        Some(PaintCache::Recording(rec)) => Some((rec.dl.clone(), rec.backdrops)),
        _ => None,
      };
      if let Some((dl, backdrops)) = cached {
        service_captures_under_cache(scene, node_id, ctx, hoist);
        // Panels baked inside the recording re-filter the live window at
        // replay, but the walk is not entering the subtree to push their
        // regions: push one in their place - the boundary's subtree
        // extent as the parent loop just wrote it this frame, so it is
        // live under transform and scroll writes the cache survives.
        match backdrops {
          boundary::BakedBackdrops::None => {}
          boundary::BakedBackdrops::Unmappable => ctx.backdrop_regions.push(None),
          boundary::BakedBackdrops::Reach(reach) => ctx.backdrop_regions.push(match element.last_extent.get() {
            cull::Extent::Bounded(rect) => Some((rect, reach)),
            _ => None,
          }),
        }
        ctx.boundaries_reused += 1;
        boundary::draw_cached_recording(builder, element, own.as_ref(), &dl, ctx.size);
        return;
      }
      // The recording outlives this frame's viewport (an ancestor scroll
      // does not invalidate it), so it must hold the whole subtree.
      let regions_before = ctx.backdrop_regions.len();
      let mut sub = DisplayListBuilder::new(None);
      let cull = ctx.cull.take();
      record_node(scene, node_id, ctx, &mut sub, hoist);
      ctx.cull = cull;
      if let Some(dl) = sub.build() {
        ctx.boundaries_recorded += 1;
        // The regions the record walk pushed (the boundary's own entry
        // predates regions_before) summarize what the cache must stand in
        // for on reuse frames.
        let backdrops = boundary::BakedBackdrops::summarize(&ctx.backdrop_regions[regions_before..]);
        boundary::draw_cached_recording(builder, element, own.as_ref(), &dl, ctx.size);
        *element.paint_cache.borrow_mut() = Some(PaintCache::Recording(boundary::RecordingCache { dl, backdrops }));
      }
    }
    BoundaryMode::Snapshot => boundary::snapshot_node(scene, node_id, ctx, builder, true),
    BoundaryMode::SnapshotNoAa => boundary::snapshot_node(scene, node_id, ctx, builder, false),
  }
}

// Services captureSnapshot requests for `node_id`: records its subtree into a
// throwaway display list at display scale (the same path snapshot_node uses to
// rasterize) and registers one exact-size texture per request. It never draws
// into the frame's builder - the node still paints normally afterwards - and
// the recording is an isolated_walk, so it does not perturb the frame being
// built.
fn service_captures<'a>(scene: &'a RenderTree, node_id: u64, ctx: &mut BuildContext<'a>, requests: Vec<CaptureDone>) {
  let element = scene.node(node_id);
  let scale = ctx.platform.display_scale();
  // The same painted box the snapshot path rasterizes into (the caller's
  // child walk set ctx.size just before recursing here), so the capture box
  // equals the painted box by construction rather than by a second size
  // derivation; the offset is countered below like the snapshot recording.
  let (width, height, offset) = boundary::painted_box(element, ctx.size);
  let (tex_w, tex_h) = ((width * scale).ceil() as u32, (height * scale).ceil() as u32);
  if tex_w == 0 || tex_h == 0 {
    let why = if element.layout.is_none() {
      "capture node is detached (d-*) with no painted size: neither its own w/h nor a laid-out ancestor gives it a box"
    } else {
      "capture node has no layout box (zero size)"
    };
    for done in requests {
      ctx.alloy.complete_capture(done, Err(why.to_string()));
    }
    return;
  }

  let own = own_matrix(element, ctx.size);
  let hoist = if own.is_some() { Hoist::Transform } else { Hoist::None };

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  if offset != (0.0, 0.0) {
    sub.translate(-offset.0, -offset.1);
  }
  isolated_walk(scene, node_id, ctx, &mut sub, hoist);

  let Some(dl) = sub.build() else {
    for done in requests {
      ctx.alloy.complete_capture(done, Err("capture produced an empty display list".to_string()));
    }
    return;
  };

  // Fresh, independent readback per request (design: each caller owns its bytes).
  for done in requests {
    let result = ctx.alloy.capture_node_pixels(&dl, tex_w, tex_h).map(|pixels| CaptureInfo {
      pixels,
      width: tex_w,
      height: tex_h,
    });
    ctx.alloy.complete_capture(done, result);
  }
}

// `hoist` names what the boundary caller applies itself at composite time
// (see Hoist); the content is recorded without those ops. A hoisted matrix is
// only ever a View's own box transform (own_matrix).
pub(super) fn record_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  hoist: Hoist,
) {
  let element = scene.node(node_id);
  let pass = ctx.backdrop_pass;
  if pass.is_none() {
    ctx.nodes_painted += 1;
    element.lifecycle.painted.set(true);
  }

  let (clip_x, clip_y) = overflow_clips(element);
  let record_clip = (clip_x || clip_y) && hoist != Hoist::Full;

  // A save is only needed for ops this recording itself carries: a recorded
  // clip, or a View's matrix/scroll (child translates below are undone
  // explicitly). Under Hoist::Full there is normally nothing to restore - a
  // design-size fit is the exception, recorded below even when hoisted.
  let view_fit = match &element.kind {
    // The fit resolves against the border box, like own_matrix; detached
    // views fall back to the inherited frame in ctx.size.
    ElementKind::View(v) => v.fit_matrix(element.frame_size(ctx.size)),
    _ => None,
  };
  let needs_save =
    record_clip || (matches!(&element.kind, ElementKind::View(_)) && (hoist != Hoist::Full || view_fit.is_some()));
  if needs_save {
    builder.save();
  }

  // Record order: user matrix, clip, scroll, fit, children - a hoist covers a
  // prefix, and draw_cached_recording applies the same order around a cached
  // fit+children recording, so the paths cannot diverge. The clip and scroll
  // both mean the layout BOX - the clip rect in box space, the scroll offset
  // in box pixels - which is why they sit under the user chain and before any
  // design-size fit (okf/backlog/overflow-viewbox-clip.md).
  if hoist == Hoist::None {
    if let Some(own) = own_matrix(element, ctx.size) {
      builder.transform(&own);
    } else if pass.is_none() {
      element.build(ctx, builder);
    }
  }
  if record_clip {
    apply_clip(builder, element);
  }
  // The backdrop layer reads the current target, so only the inline path
  // emits it here; boundary callers emit it at composite time instead
  // (boundary.rs: draw_cached_recording, BoundaryComposite) - baked into a cache
  // or a snapshot raster it would read the offscreen, not the window. A
  // backdrops-only pass emits every panel below its root at the pass alpha
  // (the root's own panel is the regular path's, emitted right after the
  // pass) and counts them.
  match ctx.backdrop_pass {
    None => {
      if hoist == Hoist::None {
        emit_backdrop(builder, element, ctx.size, 1.0);
      }
    }
    Some(pass) if pass.root != node_id => {
      if matches!(&element.kind, ElementKind::View(v) if v.active_backdrop_filter().is_some()) {
        ctx.backdrops_prepainted += 1;
      }
      emit_backdrop(builder, element, ctx.size, pass.alpha);
    }
    Some(_) => {}
  }
  if hoist != Hoist::Full {
    apply_scroll(builder, element);
  }
  if let Some(fit) = &view_fit {
    // The fit belongs to the CONTENT, recorded at every hoist level, so
    // boundary caches, snapshot textures, and captures hold fitted content
    // and the composited recording stays box-sized. set_design_size reports
    // Paint damage to match.
    builder.transform(fit);
  }

  // The window map follows the record order regardless of hoisting: a
  // hoisted matrix/scroll is applied by the caller at composite time, but
  // the content still lands under it on screen, which is what the damage
  // extents must describe. Unlike the cull rect it is never suspended
  // inside boundary recordings - a recording replays at the walk's current
  // window position this frame.
  let saved_map = ctx.to_window;
  if let Some(own) = own_matrix(element, ctx.size) {
    ctx.to_window = cull::map_through(&ctx.to_window, &own);
  }
  if let ElementKind::View(v) = &element.kind {
    if let Some(s) = v.scroll {
      ctx.to_window = cull::map_translate(&ctx.to_window, -s);
    }
  }
  if let Some(fit) = &view_fit {
    ctx.to_window = cull::map_through(&ctx.to_window, fit);
  }

  // The cull rect follows the same four ops into the child frame. Under a
  // hoist the caller applied the matrix/clip/scroll itself and reset the cull
  // (boundaries and captures hold whole subtrees), so only Hoist::None sees a
  // cull here.
  let saved_cull = ctx.cull;
  if ctx.cull.is_some() {
    if let Some(own) = own_matrix(element, ctx.size) {
      ctx.cull = ctx.cull.through(&own);
    }
    // A filter blur pulls just-offscreen content into view: widen what
    // counts as visible by its reach so that content is not culled away.
    if let Some(f) = view_filter(element) {
      let reach = f.blur_outset();
      if reach > 0.0 {
        ctx.cull = ctx.cull.map(|r| r.inflate(reach, reach));
      }
    }
    if let Some(l) = &element.layout {
      ctx.cull = ctx.cull.clipped(l.size(), clip_x, clip_y);
    }
    if let ElementKind::View(v) = &element.kind {
      if let Some(s) = v.scroll {
        ctx.cull = ctx.cull.scrolled(s);
      }
    }
    if let Some(fit) = &view_fit {
      ctx.cull = ctx.cull.through(fit);
    }
  }

  // A non-boundary View's group opacity and filter are baked here as a
  // save_layer (the paint's alpha and filters composite the children as one
  // group at the restore); boundary callers hoist both to composite time
  // instead. The bounds are a formality: Impeller intersects them with the
  // current clip coverage.
  let opacity = view_opacity(element);
  let filter = view_filter(element);
  let effect_layer = pass.is_none() && hoist == Hoist::None && (opacity < 1.0 || filter.is_some());
  if effect_layer {
    let paint = effect_paint(0.0, opacity, filter);
    let bounds = Rect::new(Point::new(-CLIP_INF, -CLIP_INF), Size::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
    builder.save_layer(&bounds, Some(&paint), None);
  }
  // A backdrops-only pass opens no layer: a fading view below the root
  // folds its opacity into the pass alpha for its subtree instead (the
  // root's own opacity is already the pass alpha), and a backdrop root
  // (cull::is_backdrop_root) ends the descent - the panels under it read
  // its own offscreen, and its own panel was emitted above.
  let (descend, child_pass) = match pass {
    None => (true, None),
    Some(p) if node_id == p.root => (true, Some(p)),
    Some(_) if cull::is_backdrop_root(element) => (false, None),
    Some(p) => (true, Some(BackdropPass { alpha: p.alpha * opacity, root: p.root })),
  };
  ctx.backdrop_pass = child_pass;

  // A text's children are spans (runs of its paragraph, drawn by the text
  // itself) and inline atoms (laid-out elements the text placed on its lines,
  // drawn like any child at their location).
  let text_atoms = matches!(&element.kind, ElementKind::Text(_));

  // The frame this node's detached children inherit (cull::child_frame is
  // the one derivation); read before the loop mutates ctx.size.
  let child_frame = cull::child_frame(element, ctx.size);

  for &child_id in &element.children {
    if !descend {
      break;
    }
    let child = scene.node(child_id);
    if child.is_hidden() {
      continue;
    }
    if text_atoms && !child.has_layout() {
      continue;
    }

    let pos = child.placement();

    // The child's current window extent, kept on the element for damage
    // resolves (see paint_phase). Written for culled children too - their
    // envelope is still their true extent - and skipped only for hidden
    // ones above, whose stale cell is exactly their to-be-erased pixels.
    // A backdrops-only pass leaves the cells to the regular walk behind it.
    let child_env = cull::envelope(scene, child_id, ctx.platform, child_frame);
    if pass.is_none() {
      child.last_extent.set(child_env.to_window(pos, &ctx.to_window));
    }

    // Viewport culling: a child whose envelope cannot reach the cull rect is
    // skipped whole. The envelope resolves against the frame the child would
    // inherit (child_frame in cull.rs mirrors the else branch below). Not
    // while a capture is pending: captures are serviced by the walk reaching
    // their node, on screen or not.
    let child_cull = ctx.cull.into_child(pos);
    if let Some(cull) = &child_cull {
      if !ctx.alloy.has_pending_captures() && !child_env.may_intersect(cull) {
        continue;
      }
    }
    let parent_cull = ctx.cull;
    ctx.cull = child_cull;
    let parent_map = ctx.to_window;
    ctx.to_window = cull::map_translate(&ctx.to_window, pos.to_vector());

    builder.translate(pos.x, pos.y);

    if child.has_layout() {
      // The child's border box, and its content box derived from the same
      // layout - the split hit testing makes too, so paint and hit size a
      // kind against the same boxes (okf/done/padding-box-divergence.md).
      let layout = child.layout.as_ref().expect("has_layout checked above");
      ctx.size = layout.size();
      ctx.content = layout.content_box();
      build_recursive(scene, child_id, ctx, builder);
    } else {
      // A detached child inherits the frame whole (the design size under a
      // design-size view, so a d-text wraps and a d-rect fills in design units);
      // no layout means no padding, so content covers the frame.
      ctx.size = child_frame;
      ctx.content = Rect::new(Point::zero(), child_frame);
      build_recursive(scene, child_id, ctx, builder);
    }

    ctx.cull = parent_cull;
    ctx.to_window = parent_map;
    builder.translate(-pos.x, -pos.y);
  }
  ctx.cull = saved_cull;
  ctx.to_window = saved_map;
  ctx.backdrop_pass = pass;

  if effect_layer {
    builder.restore();
  }
  if needs_save {
    builder.restore();
  }
}
