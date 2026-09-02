use crate::impellers::{
  ClipOperation, Color, DisplayList, DisplayListBuilder, Matrix, Paint, Point, Rect, RoundingRadii, Size, Texture,
  TextureSampling,
};
use taffy::style::Overflow;
use taffy::{AvailableSpace, NodeId};

use crate::rendertree::cull::{self, CullRect};
use crate::rendertree::{
  BoundaryMode, BuildContext, Element, ElementKind, FilterState, FrameDamage, LayoutContext, PaintCache,
  PlatformContext, RenderTree, ShadedCache, SnapshotCache,
};
use crate::{CaptureDone, CaptureInfo};

// Large finite extent used to leave one axis effectively unclipped when only the
// other axis has non-visible overflow. clip_rect requires a finite rectangle.
const CLIP_INF: f32 = 1.0e7;

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
  let mut layout_ctx = LayoutContext { render_tree: tree, platform, alloy };
  taffy::compute_root_layout(&mut layout_ctx, NodeId::from(root_id), available_space);
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
  let (damaged, damage_full) = tree.take_damage();
  let size = Size::new(width, height);
  let window_rect = Rect::new(Point::zero(), size);
  tree.node(root_id).last_extent.set(cull::Extent::Bounded(window_rect));

  // The walk borrows the tree for the BuildContext's lifetime, so it is
  // scoped: only plain stats leave the block.
  let (mut stats, damage) = {
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
      damage_px: 0.0,
    };
    (stats, damage)
  };
  let frame_damage = clamp_damage(damage, damage_full, window_rect);
  tree.set_frame_damage(frame_damage);
  stats.damage_px = frame_damage.area(size);
  // Any capture request whose node the walk never visited targets a node that
  // is not in the live tree; fail it rather than leave its promise pending.
  alloy.fail_unserviced_captures();
  // Deliver every capture outcome now the walk is done, so callbacks (which may
  // read back or free textures) run out of the tree borrow.
  alloy.deliver_captures();
  // Vended snapshot textures whose boundary was deleted join the deferred
  // destroys below, so a consumer still sampling one keeps its last pixels.
  for id in tree.take_released_snapshot_textures() {
    alloy.release_borrowed(id);
  }
  // Deferred destroys: reclaim ids the live tree no longer references. This
  // frame's display list is already recorded (Rc'd Impeller handles keep its
  // textures alive), and any still-referenced id stays queued so a build never
  // finds a hole in the registry. Gated so the tree scan only runs when a
  // destroy is actually pending.
  if alloy.has_pending_destroys() {
    alloy.reclaim_destroyed(&tree.referenced_texture_ids());
  }
  stats
}

// A resolved damage union cut to the window and to the FrameDamage form.
fn clamp_damage(damage: cull::Extent, full: bool, window_rect: Rect) -> FrameDamage {
  if full {
    return FrameDamage::Full;
  }
  match damage {
    cull::Extent::Empty => FrameDamage::None,
    cull::Extent::Unbounded => FrameDamage::Full,
    cull::Extent::Bounded(r) => match r.intersection(&window_rect) {
      // Damage entirely outside the window changes no visible pixel.
      None => FrameDamage::None,
      Some(clamped) if clamped.contains_rect(&window_rect) => FrameDamage::Full,
      Some(clamped) => FrameDamage::Rect(clamped),
    },
  }
}

/// Resolve the accumulated damage WITHOUT a paint walk, for the present-only
/// reuse path (PendingFrame::commit): the tree is unchanged since the last
/// walk, so a damaged id's cell is both its old and its new extent. Only
/// GPU-content damage lands here - texture nodes whose pixels changed behind
/// an unchanged display list (texture_content_changed notes them without a
/// revision bump).
pub fn resolve_reuse_damage(tree: &mut RenderTree, window: Size) -> FrameDamage {
  let (damaged, full) = tree.take_damage();
  let window_rect = Rect::new(Point::zero(), window);
  let mut damage = cull::Extent::Empty;
  if !full {
    for &id in &damaged {
      damage = damage.union(damaged_extent(tree, id));
    }
  }
  clamp_damage(damage, full, window_rect)
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
  // GPU content writes since the last frame (target re-renders, uploads,
  // camera frames) change pixels behind unchanged texture ids and leave no
  // tree damage of their own; a baked snapshot boundary over one would keep
  // replaying stale pixels. The runner's frame loop drains this itself,
  // ahead of its display-list reuse check; this path is the frame producer
  // for everything else, so it must apply them too.
  let content = alloy.take_content_changes();
  if !content.is_empty() {
    tree.texture_content_changed(&content);
  }

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

// What a boundary caller applies itself at composite time, and record_node
// therefore leaves out of the cached content. The record order is matrix,
// clip, scroll, fit, children; a hoist always covers a prefix of the first
// three (a hoisted scroll requires a hoisted clip, otherwise the
// composite-time scroll translate would move a recorded clip that must stay
// put in viewport space; a design-size fit is never hoisted - it is content).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hoist {
  /// Record everything (non-boundary nodes, non-View boundaries).
  None,
  /// The caller applies the View's matrix; clip and scroll stay recorded.
  /// Snapshot boundaries use this: their raster must bake clip and scroll,
  /// since the texture holds only the pixels visible at rasterize time.
  Transform,
  /// The caller applies matrix, clip and scroll; the cache holds the fit and
  /// children only. Recording boundaries use this, making the cache reusable
  /// under scroll writes as well as transform writes (see Damage::Scroll).
  Full,
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
fn own_matrix(element: &Element, inherited: Size) -> Option<Matrix> {
  match &element.kind {
    ElementKind::View(v) => {
      let box_size = element.layout.as_ref().map(|l| l.size()).unwrap_or(inherited);
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
fn apply_clip(builder: &mut DisplayListBuilder, element: &Element) {
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
fn apply_scroll(builder: &mut DisplayListBuilder, element: &Element) {
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
fn view_opacity(element: &Element) -> f32 {
  match &element.kind {
    ElementKind::View(v) => v.opacity.unwrap_or(1.0),
    _ => 1.0,
  }
}

// A View's subtree filter; non-Views (and empty declarations) have none.
// Hoisted like opacity: applied around cached content at composite time, so
// a filter write replays the same cache.
fn view_filter(element: &Element) -> Option<&FilterState> {
  match &element.kind {
    ElementKind::View(v) => v.active_filter(),
    _ => None,
  }
}

// The paint carrying a view's composite-time effects: the group opacity in
// the alpha (riding on `rgb` - black for a save_layer, white for a texture
// quad, where white keeps the texture's colors), plus the filter's fused
// color matrix and blur.
fn effect_paint(rgb: f32, opacity: f32, filter: Option<&FilterState>) -> Paint {
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

// Replays a recorded display list under the view's composite-time effects: a
// filter needs a save_layer carrying it (draw_display_list has only an
// opacity argument), plain opacity stays on the cheap path.
fn draw_dl_with_effects(builder: &mut DisplayListBuilder, dl: &DisplayList, opacity: f32, filter: Option<&FilterState>) {
  if filter.is_some() {
    let paint = effect_paint(0.0, opacity, filter);
    let bounds = Rect::new(Point::new(-CLIP_INF, -CLIP_INF), Size::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
    builder.save_layer(&bounds, Some(&paint), None);
    builder.draw_display_list(dl, 1.0);
    builder.restore();
  } else {
    builder.draw_display_list(dl, opacity);
  }
}

// Composites a Recording boundary's cached content. A View boundary's cache
// holds children only (Hoist::Full): its current matrix, clip and scroll are
// applied around the draw here, so transform and scroll writes replay the
// same cache. A non-View boundary's cache holds everything and draws bare.
fn draw_cached_recording(
  builder: &mut DisplayListBuilder,
  element: &Element,
  matrix: Option<&Matrix>,
  dl: &DisplayList,
) {
  let opacity = view_opacity(element);
  let filter = view_filter(element);
  if let Some(m) = matrix {
    builder.save();
    builder.transform(m);
    apply_clip(builder, element);
    apply_scroll(builder, element);
    draw_dl_with_effects(builder, dl, opacity, filter);
    builder.restore();
  } else {
    draw_dl_with_effects(builder, dl, opacity, filter);
  }
}

// Applies a boundary's hoisted matrix around `draw`; a plain pass-through when
// the boundary has no transform of its own.
fn draw_with_transform(
  builder: &mut DisplayListBuilder,
  matrix: Option<&Matrix>,
  draw: impl FnOnce(&mut DisplayListBuilder),
) {
  if let Some(m) = matrix {
    builder.save();
    builder.transform(m);
    draw(builder);
    builder.restore();
  } else {
    draw(builder);
  }
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
  match element.repaint_boundary {
    BoundaryMode::None => record_node(scene, node_id, ctx, builder, Hoist::None),
    BoundaryMode::Recording => {
      let own = own_matrix(element, ctx.size);
      let hoist = if own.is_some() { Hoist::Full } else { Hoist::None };
      let cached = match &*element.paint_cache.borrow() {
        Some(PaintCache::Recording(dl)) => Some(dl.clone()),
        _ => None,
      };
      if let Some(dl) = cached {
        ctx.boundaries_reused += 1;
        draw_cached_recording(builder, element, own.as_ref(), &dl);
        return;
      }
      // The recording outlives this frame's viewport (an ancestor scroll
      // does not invalidate it), so it must hold the whole subtree.
      let mut sub = DisplayListBuilder::new(None);
      let cull = ctx.cull.take();
      record_node(scene, node_id, ctx, &mut sub, hoist);
      ctx.cull = cull;
      if let Some(dl) = sub.build() {
        ctx.boundaries_recorded += 1;
        draw_cached_recording(builder, element, own.as_ref(), &dl);
        *element.paint_cache.borrow_mut() = Some(PaintCache::Recording(dl));
      }
    }
    BoundaryMode::Snapshot => snapshot_node(scene, node_id, ctx, builder, true),
    BoundaryMode::SnapshotNoAa => snapshot_node(scene, node_id, ctx, builder, false),
  }
}

// Snapshot gate: the subtree is rasterized into a texture at the current
// display scale and composited as a single quad until something inside it
// changes, its layout size changes, or the display scale changes. Content
// painting outside the layout box is cropped (unlike a recording boundary);
// the crop happens in untransformed local space, since the boundary's own
// transform is hoisted out of the raster and applied to the quad instead.
// All storage is exact-size. A declared boundary shader adds one pass over
// the rasterization and composites its output instead (see View::set_shader).
fn snapshot_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  aa: bool,
) {
  // The texture outlives this frame's viewport (an ancestor scroll does not
  // invalidate it), so the raster must hold the whole subtree.
  let cull = ctx.cull.take();
  snapshot_node_uncalled(scene, node_id, ctx, builder, aa);
  ctx.cull = cull;
}

fn snapshot_node_uncalled<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  aa: bool,
) {
  let element = scene.node(node_id);
  // A laid-out node snapshots its layout box. A detached (d-*) node has none,
  // but it is still drawn into a definite rectangle: its kind's painted box,
  // sized with the same ctx.size its build() reads (the same derivation as
  // service_captures, so snapshot and capture box the node identically). The
  // box's x/y is the node's own paint offset, countered in the recording so
  // the content lands at the texture origin and restored on the composited
  // quad's dst - except for a View, whose offset (translate) lives in the
  // matrix that Hoist::Transform keeps out of the recording anyway.
  let (width, height, offset) = match element.layout.as_ref() {
    Some(l) => (l.size().width, l.size().height, (0.0, 0.0)),
    None => {
      let local = element.kind.local_bounds(ctx.size);
      let offset = match &element.kind {
        ElementKind::View(_) => (0.0, 0.0),
        _ => (local.origin.x, local.origin.y),
      };
      (local.size.width, local.size.height, offset)
    }
  };
  let scale = ctx.platform.display_scale();
  let (tex_w, tex_h) = ((width * scale).ceil() as u32, (height * scale).ceil() as u32);

  // Without a positive painted box there is nothing to rasterize into; paint
  // inline so overflowing content still shows up.
  if tex_w == 0 || tex_h == 0 {
    record_node(scene, node_id, ctx, builder, Hoist::None);
    return;
  }

  let own = own_matrix(element, ctx.size);
  let hoist = if own.is_some() { Hoist::Transform } else { Hoist::None };

  // Group opacity and the view filter ride on the composited quad (white
  // keeps the texture's colors, the alpha fades it, the filters transform
  // the draw), so the texture itself stays effect-free and survives opacity
  // and filter writes.
  let opacity = view_opacity(element);
  let filter = view_filter(element);
  let quad_paint = (opacity < 1.0 || filter.is_some()).then(|| effect_paint(1.0, opacity, filter));

  // The content occupies the top-left width*scale x height*scale pixels of the
  // (ceil-padded) texture; mapping exactly that region onto the logical-size
  // quad keeps the composite pixel-exact under the root scale transform. The
  // quad sits at the detached paint offset the recording countered.
  let src = Rect::new(Point::new(0.0, 0.0), Size::new(width * scale, height * scale));
  let dst = Rect::new(Point::new(offset.0, offset.1), Size::new(width, height));

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
    // size, so src/dst and the pixel dims are re-derived here.
    let outset = decl.outset.max(0.0);
    let (canvas_w, canvas_h) = (width + 2.0 * outset, height + 2.0 * outset);
    let (tex_w, tex_h) = ((canvas_w * scale).ceil() as u32, (canvas_h * scale).ceil() as u32);
    let src = Rect::new(Point::new(0.0, 0.0), Size::new(canvas_w * scale, canvas_h * scale));
    let dst = Rect::new(Point::new(-outset, -outset), Size::new(canvas_w, canvas_h));

    // Valid content with matching shader storage: composite the cached
    // output, re-running the pass in place first when a declaration write
    // is pending (the params path - the snapshot is not re-rasterized). An
    // outset change or a `previous` toggle fails the compare instead; both
    // change what storage must exist.
    let cached = {
      let cache = element.paint_cache.borrow();
      match &*cache {
        Some(PaintCache::Snapshot(snap)) => match &snap.shaded {
          Some(sc)
            if snap.valid
              && snap.width == width
              && snap.height == height
              && snap.scale == scale
              && sc.outset == outset
              && sc.history.is_some() == decl.previous =>
          {
            Some((snap.texture.clone(), sc.output.clone(), sc.history.clone()))
          }
          _ => None,
        },
        _ => None,
      }
    };
    if let Some((source, output, history)) = cached {
      if shader_dirty {
        if let Err(e) = ctx.alloy.rerun_node_shader(decl, &source, &output, history.as_ref(), tex_w, tex_h) {
          log::warn!("boundary shader re-run failed for node {node_id}: {e}");
        }
      }
      ctx.snapshots_reused += 1;
      draw_with_transform(builder, own.as_ref(), |b| {
        b.draw_texture_rect(&output, &src, &dst, TextureSampling::Linear, quad_paint.as_ref());
      });
      return;
    }

    // Content changed (or the declaration needs different storage): record,
    // rasterize and run the pass in one trip. Dimension-matched storage is
    // re-rendered in place; exact storage means only an exact match
    // qualifies, which the (width, height, scale) + outset equality is.
    let mut sub = DisplayListBuilder::new(None);
    sub.scale(scale, scale);
    if outset > 0.0 {
      sub.translate(outset, outset);
      // Without an outset the box crop is the texture viewport itself; with
      // a margin the crop must be explicit, or overflowing content would
      // paint into the effect's transparent margin.
      sub.clip_rect(&Rect::new(Point::new(0.0, 0.0), Size::new(width, height)), ClipOperation::Intersect);
    }
    record_node(scene, node_id, ctx, &mut sub, hoist);
    let Some(dl) = sub.build() else { return };

    // Reusable storage: the source (plain or shaded), plus output and
    // history when the cache was already shaded. A plain cache's texture
    // counts as outset 0, so declaring a no-outset shader over an existing
    // snapshot re-renders its storage instead of reallocating.
    let retained = {
      let cache = element.paint_cache.borrow();
      match &*cache {
        Some(PaintCache::Snapshot(snap))
          if snap.width == width
            && snap.height == height
            && snap.scale == scale
            && snap.shaded.as_ref().map_or(0.0, |sc| sc.outset) == outset =>
        {
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
        draw_with_transform(builder, own.as_ref(), |b| {
          b.draw_texture_rect(&output, &src, &dst, TextureSampling::Linear, quad_paint.as_ref());
        });
        *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot(SnapshotCache {
          texture: source,
          width,
          height,
          scale,
          valid: true,
          shaded: Some(ShadedCache { output, outset, history }),
        }));
      }
      Err(e) => {
        // Paint inline (unshaded) this frame and drop the cache, so no
        // stale storage is offered for in-place reuse on the next damage.
        // The recording carries its own device-scale transform and content
        // offset, so counter both before replaying.
        log::warn!("shaded snapshot failed for node {node_id}: {e}; painting inline unshaded");
        element.paint_cache.borrow_mut().take();
        draw_with_transform(builder, own.as_ref(), |b| {
          b.save();
          b.translate(-outset, -outset);
          b.scale(1.0 / scale, 1.0 / scale);
          draw_dl_with_effects(b, &dl, opacity, filter);
          b.restore();
        });
      }
    }
    return;
  }

  {
    let cache = element.paint_cache.borrow();
    if let Some(PaintCache::Snapshot(snap)) = &*cache {
      if snap.valid && snap.width == width && snap.height == height && snap.scale == scale {
        ctx.snapshots_reused += 1;
        draw_with_transform(builder, own.as_ref(), |b| {
          b.draw_texture_rect(&snap.texture, &src, &dst, TextureSampling::Linear, quad_paint.as_ref());
        });
        return;
      }
    }
  }

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  if offset != (0.0, 0.0) {
    sub.translate(-offset.0, -offset.1);
  }
  record_node(scene, node_id, ctx, &mut sub, hoist);
  let Some(dl) = sub.build() else { return };

  // Stale storage at unchanged dimensions is re-rendered in place: the
  // offscreen draw clears and rewrites the full allocation, so no stale
  // pixels survive. Exact-size storage means any dimension change
  // reallocates.
  let retained = {
    let cache = element.paint_cache.borrow();
    match &*cache {
      Some(PaintCache::Snapshot(snap)) if snap.width == width && snap.height == height && snap.scale == scale => {
        Some(snap.texture.clone())
      }
      _ => None,
    }
  };
  if let Some(texture) = retained {
    match ctx.alloy.render_display_list_into_texture(&dl, &texture, tex_w, tex_h, aa) {
      Ok(()) => {
        ctx.snapshots_rerendered += 1;
        publish_snapshot(element, ctx, &texture, tex_w, tex_h);
        draw_with_transform(builder, own.as_ref(), |b| {
          b.draw_texture_rect(&texture, &src, &dst, TextureSampling::Linear, quad_paint.as_ref());
        });
        *element.paint_cache.borrow_mut() =
          Some(PaintCache::Snapshot(SnapshotCache { texture, width, height, scale, valid: true, shaded: None }));
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
      draw_with_transform(builder, own.as_ref(), |b| {
        b.draw_texture_rect(&texture, &src, &dst, TextureSampling::Linear, quad_paint.as_ref());
      });
      *element.paint_cache.borrow_mut() =
        Some(PaintCache::Snapshot(SnapshotCache { texture, width, height, scale, valid: true, shaded: None }));
    }
    Err(e) => {
      // Paint inline this frame; the recording carries its own device-scale
      // transform and detached paint offset, so counter both before replaying.
      log::warn!("snapshot rasterization failed for node {node_id}: {e}; painting inline");
      draw_with_transform(builder, own.as_ref(), |b| {
        b.save();
        b.translate(offset.0, offset.1);
        b.scale(1.0 / scale, 1.0 / scale);
        draw_dl_with_effects(b, &dl, opacity, filter);
        b.restore();
      });
    }
  }
}

// Re-point a boundary's vended texture id (see RenderTree::snapshot_texture)
// at the rasterization just produced. A boundary nobody asked for publishes
// nothing.
fn publish_snapshot(element: &Element, ctx: &BuildContext<'_>, texture: &Texture, tex_w: u32, tex_h: u32) {
  if let Some(id) = element.snapshot_texture_id.get() {
    ctx.alloy.publish_snapshot_texture(id, texture, tex_w, tex_h);
  }
}

// Services captureSnapshot requests for `node_id`: records its subtree into a
// throwaway display list at display scale (the same path snapshot_node uses to
// rasterize) and registers one exact-size texture per request. It never draws
// into the frame's builder - the node still paints normally afterwards - and it
// isolates the shared ctx (size and boundary stats) so the recording it does
// here does not perturb the frame being built.
fn service_captures<'a>(scene: &'a RenderTree, node_id: u64, ctx: &mut BuildContext<'a>, requests: Vec<CaptureDone>) {
  let element = scene.node(node_id);
  let scale = ctx.platform.display_scale();
  // A laid-out node captures its layout box. A detached (d-*) node has none,
  // but it is still drawn into a definite rectangle: its kind's painted box,
  // sized with the same ctx.size its build() reads (the caller's child walk
  // set it just before recursing here), so the capture box equals the painted
  // box by construction rather than by a second size derivation. The box's
  // x/y is the node's own paint offset, countered below so the content lands
  // at the texture origin - except for a View, whose offset (translate) lives
  // in the matrix that Hoist::Transform keeps out of the recording anyway.
  let (width, height, offset) = match element.layout.as_ref() {
    Some(l) => (l.size().width, l.size().height, (0.0, 0.0)),
    None => {
      let local = element.kind.local_bounds(ctx.size);
      let offset = match &element.kind {
        ElementKind::View(_) => (0.0, 0.0),
        _ => (local.origin.x, local.origin.y),
      };
      (local.size.width, local.size.height, offset)
    }
  };
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

  let saved_size = ctx.size;
  let saved_content = ctx.content;
  // A capture holds the whole subtree, not the on-screen part of it.
  let saved_cull = ctx.cull.take();
  let saved_stats = (
    ctx.boundaries_reused,
    ctx.boundaries_recorded,
    ctx.snapshots_reused,
    ctx.snapshots_rerendered,
    ctx.snapshots_rasterized,
  );
  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  if offset != (0.0, 0.0) {
    sub.translate(-offset.0, -offset.1);
  }
  record_node(scene, node_id, ctx, &mut sub, hoist);
  ctx.size = saved_size;
  ctx.content = saved_content;
  ctx.cull = saved_cull;
  ctx.boundaries_reused = saved_stats.0;
  ctx.boundaries_recorded = saved_stats.1;
  ctx.snapshots_reused = saved_stats.2;
  ctx.snapshots_rerendered = saved_stats.3;
  ctx.snapshots_rasterized = saved_stats.4;

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
fn record_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  hoist: Hoist,
) {
  let element = scene.node(node_id);
  ctx.nodes_painted += 1;

  let (clip_x, clip_y) = overflow_clips(element);
  let record_clip = (clip_x || clip_y) && hoist != Hoist::Full;

  // A save is only needed for ops this recording itself carries: a recorded
  // clip, or a View's matrix/scroll (child translates below are undone
  // explicitly). Under Hoist::Full there is normally nothing to restore - a
  // design-size fit is the exception, recorded below even when hoisted.
  let view_fit = match &element.kind {
    // The fit resolves against the border box, like own_matrix; detached
    // views fall back to the inherited frame in ctx.size.
    ElementKind::View(v) => v.fit_matrix(element.layout.as_ref().map(|l| l.size()).unwrap_or(ctx.size)),
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
    } else {
      element.build(ctx, builder);
    }
  }
  if record_clip {
    apply_clip(builder, element);
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
  let effect_layer = hoist == Hoist::None && (opacity < 1.0 || filter.is_some());
  if effect_layer {
    let paint = effect_paint(0.0, opacity, filter);
    let bounds = Rect::new(Point::new(-CLIP_INF, -CLIP_INF), Size::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
    builder.save_layer(&bounds, Some(&paint), None);
  }

  // A text's children are spans (runs of its paragraph, drawn by the text
  // itself) and inline atoms (laid-out elements the text placed on its lines,
  // drawn like any child at their location).
  let text_atoms = matches!(&element.kind, ElementKind::Text(_));

  // The frame this node's detached children inherit (cull::child_frame is
  // the one derivation); read before the loop mutates ctx.size.
  let child_frame = cull::child_frame(element, ctx.size);

  for &child_id in &element.children {
    let child = scene.node(child_id);
    if child.is_hidden() {
      continue;
    }
    if text_atoms && !child.has_layout() {
      continue;
    }

    let pos = child.layout.as_ref().map(|l| l.location()).unwrap_or_default();

    // The child's current window extent, kept on the element for damage
    // resolves (see paint_phase). Written for culled children too - their
    // envelope is still their true extent - and skipped only for hidden
    // ones above, whose stale cell is exactly their to-be-erased pixels.
    let child_env = cull::envelope(scene, child_id, ctx.platform, child_frame);
    child.last_extent.set(child_env.to_window(pos, &ctx.to_window));

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

  if effect_layer {
    builder.restore();
  }
  if needs_save {
    builder.restore();
  }
}
