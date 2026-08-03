use crate::impellers::{
  ClipOperation, Color, DisplayList, DisplayListBuilder, Matrix, Paint, Point, Rect, RoundingRadii, Size,
  TextureSampling,
};
use taffy::style::Overflow;
use taffy::{AvailableSpace, NodeId};

use crate::rendertree::{
  BoundaryMode, BuildContext, Element, ElementKind, LayoutContext, PaintCache, PlatformContext, RenderTree,
  ShadedCache, SnapshotCache,
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
  pub boundaries_reused: u32,
  pub boundaries_recorded: u32,
  pub snapshots_reused: u32,
  pub snapshots_rerendered: u32,
  pub snapshots_rasterized: u32,
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

  let mut ctx = BuildContext::new(platform, alloy);
  ctx.size = Size::new(width, height);
  build_recursive(tree, root_id, &mut ctx, builder);
  // Any capture request whose node the walk never visited targets a node that
  // is not in the live tree; fail it rather than leave its promise pending.
  alloy.fail_unserviced_captures();
  // Deliver every capture outcome now the walk is done, so callbacks (which may
  // read back or free textures) run out of the tree borrow.
  alloy.deliver_captures();
  // Deferred destroys: reclaim ids the live tree no longer references. This
  // frame's display list is already recorded (Rc'd Impeller handles keep its
  // textures alive), and any still-referenced id stays queued so a build never
  // finds a hole in the registry. Gated so the tree scan only runs when a
  // destroy is actually pending.
  if alloy.has_pending_destroys() {
    alloy.reclaim_destroyed(&tree.referenced_texture_ids());
  }
  PaintStats {
    boundaries_reused: ctx.boundaries_reused,
    boundaries_recorded: ctx.boundaries_recorded,
    snapshots_reused: ctx.snapshots_reused,
    snapshots_rerendered: ctx.snapshots_rerendered,
    snapshots_rasterized: ctx.snapshots_rasterized,
  }
}

/// Lay out and paint the whole tree into a fresh display list and submit it to
/// the render thread: the minimal "draw the tree once" path. Unlike the runner's
/// frame loop it carries no demand gating, retained-list reuse, post-layout hook
/// or stats overlay (all of which are policy); it is what a flux + alloy app
/// calls each frame to put the current tree on screen. Returns the frame's paint
/// stats.
pub fn render(tree: &mut RenderTree, platform: &PlatformContext, alloy: &crate::Context) -> PaintStats {
  let mut builder = DisplayListBuilder::new(None);
  let scale = platform.display_scale();
  builder.scale(scale, scale);
  let stats = paint_phase(&mut builder, tree, platform, alloy);
  if let Some(dl) = builder.build() {
    if alloy.submit(dl).is_err() {
      log::warn!("rendertree::render: render thread unavailable, dropping frame");
    }
  }
  stats
}

// What a boundary caller applies itself at composite time, and record_node
// therefore leaves out of the cached content. The record order is matrix,
// clip, scroll, children; a hoist always covers a prefix of that order (a
// hoisted scroll requires a hoisted clip, otherwise the composite-time scroll
// translate would move a recorded clip that must stay put in viewport space).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hoist {
  /// Record everything (non-boundary nodes, non-View boundaries).
  None,
  /// The caller applies the View's matrix; clip and scroll stay recorded.
  /// Snapshot boundaries use this: their raster must bake clip and scroll,
  /// since the texture holds only the pixels visible at rasterize time.
  Transform,
  /// The caller applies matrix, clip and scroll; the cache holds children
  /// only. Recording boundaries use this, making the cache reusable under
  /// scroll writes as well as transform writes (see Damage::Scroll).
  Full,
}

// A boundary View's own transform is hoisted out of its cached content: the
// recording/texture holds the content in untransformed local space, and the
// current matrix is applied around the cached draw at composite time. This
// keeps the cache reusable under transform-only changes, and stops Snapshot
// mode baking a rotation/scale into the layout-box crop. Non-View kinds paint
// transform-free content in build(), so there is nothing to hoist.
fn hoisted_matrix(element: &Element, size: Size) -> Option<Matrix> {
  match &element.kind {
    // The user chain only: a viewBox fit is content, recorded into the cache
    // by record_node, so the composited quad/recording is box-sized and the
    // fit is never applied twice.
    ElementKind::View(v) => Some(v.box_matrix(size)),
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
// node-local, pre-scroll space. Shared by record_node and the Recording
// boundary composite path so the two cannot diverge. No-op without a clip.
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

// Scroll offset: applied after the clip so the clip box stays put in viewport
// space while children slide under it. Positive scroll shifts content
// leftward/upward. No-op for non-Views and unscrolled Views.
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
  if let Some(m) = matrix {
    builder.save();
    builder.transform(m);
    apply_clip(builder, element);
    apply_scroll(builder, element);
    builder.draw_display_list(dl, opacity);
    builder.restore();
  } else {
    builder.draw_display_list(dl, opacity);
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
      let own = hoisted_matrix(element, ctx.size);
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
      let mut sub = DisplayListBuilder::new(None);
      record_node(scene, node_id, ctx, &mut sub, hoist);
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
  let element = scene.node(node_id);
  let size = element.layout.as_ref().map(|l| l.size()).unwrap_or_default();
  let (width, height) = (size.width, size.height);
  let scale = ctx.platform.display_scale();
  let (tex_w, tex_h) = ((width * scale).ceil() as u32, (height * scale).ceil() as u32);

  // Without a real layout box there is nothing to rasterize into; paint
  // inline so overflowing content still shows up.
  if tex_w == 0 || tex_h == 0 {
    record_node(scene, node_id, ctx, builder, Hoist::None);
    return;
  }

  let own = hoisted_matrix(element, ctx.size);
  let hoist = if own.is_some() { Hoist::Transform } else { Hoist::None };

  // Group opacity rides on the composited quad (white keeps the texture's
  // colors, the alpha fades it), so the texture itself stays opacity-free and
  // survives opacity writes.
  let opacity = view_opacity(element);
  let opacity_paint = (opacity < 1.0).then(|| {
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 1.0, 1.0, opacity));
    paint
  });

  // The content occupies the top-left width*scale x height*scale pixels of the
  // (ceil-padded) texture; mapping exactly that region onto the logical-size
  // quad keeps the composite pixel-exact under the root scale transform.
  let src = Rect::new(Point::new(0.0, 0.0), Size::new(width * scale, height * scale));
  let dst = Rect::new(Point::new(0.0, 0.0), Size::new(width, height));

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
        b.draw_texture_rect(&output, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
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
        draw_with_transform(builder, own.as_ref(), |b| {
          b.draw_texture_rect(&output, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
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
          b.draw_display_list(&dl, opacity);
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
          b.draw_texture_rect(&snap.texture, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
        });
        return;
      }
    }
  }

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
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
        draw_with_transform(builder, own.as_ref(), |b| {
          b.draw_texture_rect(&texture, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
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
      draw_with_transform(builder, own.as_ref(), |b| {
        b.draw_texture_rect(&texture, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
      });
      *element.paint_cache.borrow_mut() =
        Some(PaintCache::Snapshot(SnapshotCache { texture, width, height, scale, valid: true, shaded: None }));
    }
    Err(e) => {
      // Paint inline this frame; the recording carries its own device-scale
      // transform, so counter the enclosing CTM's scale before replaying.
      log::warn!("snapshot rasterization failed for node {node_id}: {e}; painting inline");
      draw_with_transform(builder, own.as_ref(), |b| {
        b.save();
        b.scale(1.0 / scale, 1.0 / scale);
        b.draw_display_list(&dl, opacity);
        b.restore();
      });
    }
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

  let own = hoisted_matrix(element, ctx.size);
  let hoist = if own.is_some() { Hoist::Transform } else { Hoist::None };

  let saved_size = ctx.size;
  let saved_stats =
    (ctx.boundaries_reused, ctx.boundaries_recorded, ctx.snapshots_reused, ctx.snapshots_rerendered, ctx.snapshots_rasterized);
  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  if offset != (0.0, 0.0) {
    sub.translate(-offset.0, -offset.1);
  }
  record_node(scene, node_id, ctx, &mut sub, hoist);
  ctx.size = saved_size;
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

  // Fresh, independent texture per request (design: a new id per call).
  for done in requests {
    let result = ctx
      .alloy
      .capture_node_texture(&dl, tex_w, tex_h)
      .map(|texture_id| CaptureInfo { texture_id, width: tex_w, height: tex_h });
    ctx.alloy.complete_capture(done, result);
  }
}

// `hoist` names what the boundary caller applies itself at composite time
// (see Hoist); the content is recorded without those ops. A hoisted matrix is
// only ever a View's, whose build() is exactly that matrix concat.
fn record_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
  hoist: Hoist,
) {
  let element = scene.node(node_id);

  let (clip_x, clip_y) = overflow_clips(element);
  let record_clip = (clip_x || clip_y) && hoist != Hoist::Full;

  // A save is only needed for ops this recording itself carries: a recorded
  // clip, or a View's matrix/scroll (child translates below are undone
  // explicitly). Under Hoist::Full there is normally nothing to restore - a
  // viewBox fit is the exception, recorded below even when hoisted.
  let view_fit = match &element.kind {
    ElementKind::View(v) => v.fit_matrix(ctx.size),
    _ => None,
  };
  let needs_save =
    record_clip || (matches!(&element.kind, ElementKind::View(_)) && (hoist != Hoist::Full || view_fit.is_some()));
  if needs_save {
    builder.save();
  }

  if hoist == Hoist::None {
    element.build(ctx, builder);
  } else if let Some(fit) = &view_fit {
    // The hoisted matrix is only the user chain (box_matrix); the viewBox fit
    // belongs to the content, so boundary caches, snapshot textures, and
    // captures hold fitted content. set_view_box reports Paint damage to
    // match.
    builder.transform(fit);
  }

  // A non-boundary View's group opacity is baked here as a save_layer (the
  // alpha composites the children as one group at the restore); boundary
  // callers hoist it to composite time instead. The bounds are a formality:
  // Impeller intersects them with the current clip coverage.
  let opacity = view_opacity(element);
  let opacity_layer = hoist == Hoist::None && opacity < 1.0;
  if opacity_layer {
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, opacity));
    let bounds = Rect::new(Point::new(-CLIP_INF, -CLIP_INF), Size::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
    builder.save_layer(&bounds, Some(&paint), None);
  }

  if record_clip {
    apply_clip(builder, element);
  }
  if hoist != Hoist::Full {
    apply_scroll(builder, element);
  }

  // Text children are Spans - not visual, skip recursion
  if let ElementKind::Text(_) = &element.kind {
    if needs_save {
      builder.restore();
    }
    return;
  }

  for &child_id in &element.children {
    let child = scene.node(child_id);

    let pos = child.layout.as_ref().map(|l| l.location()).unwrap_or_default();

    builder.translate(pos.x, pos.y);

    if child.has_layout() {
      let layout = &child.layout.as_ref().unwrap().computed;
      let pad_left = layout.padding.left;
      let pad_top = layout.padding.top;
      let pad_right = layout.padding.right;
      let pad_bottom = layout.padding.bottom;

      ctx.size.width = layout.size.width - pad_left - pad_right;
      ctx.size.height = layout.size.height - pad_top - pad_bottom;

      build_recursive(scene, child_id, ctx, builder);
    } else {
      if let Some(layout) = &element.layout {
        ctx.size = layout.size();
      }
      // Children of a viewBox view draw in the design space: the box they
      // inherit is the design size (the fit maps it onto the layout box), so
      // a d-text wraps and a d-rect fills in design units.
      if let ElementKind::View(v) = &element.kind {
        if let Some(vb) = v.view_box {
          ctx.size = vb;
        }
      }
      build_recursive(scene, child_id, ctx, builder);
    }

    builder.translate(-pos.x, -pos.y);
  }

  if opacity_layer {
    builder.restore();
  }
  if needs_save {
    builder.restore();
  }
}
