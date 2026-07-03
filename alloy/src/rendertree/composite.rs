use crate::impellers::{
  ClipOperation, Color, DisplayList, DisplayListBuilder, Matrix, Paint, Point as IPoint, Rect, RoundingRadii,
  Size as ISize, TextureSampling,
};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::Point;

use crate::rendertree::{
  BoundaryMode, BuildContext, Element, ElementKind, LayoutContext, PaintCache, PlatformContext, RenderTree, WH,
};

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

  let available_space = Size { width: AvailableSpace::Definite(width), height: AvailableSpace::Definite(height) };
  let mut layout_ctx = LayoutContext { render_tree: tree, platform, alloy };
  taffy::compute_root_layout(&mut layout_ctx, NodeId::from(root_id), available_space);
}

/// Repaint-boundary counts for one painted frame: subtrees drawn from their
/// retained recording vs freshly recorded, and snapshot boundaries drawn from
/// their retained texture vs freshly rasterized.
#[derive(Clone, Copy, Default)]
pub struct PaintStats {
  pub boundaries_reused: u32,
  pub boundaries_recorded: u32,
  pub snapshots_reused: u32,
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
  ctx.size = WH::new(width, height);
  build_recursive(tree, root_id, &mut ctx, builder);
  PaintStats {
    boundaries_reused: ctx.boundaries_reused,
    boundaries_recorded: ctx.boundaries_recorded,
    snapshots_reused: ctx.snapshots_reused,
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
fn hoisted_matrix(element: &Element, size: WH) -> Option<Matrix> {
  match &element.kind {
    ElementKind::View(v) => Some(v.paint_matrix(size)),
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
  let w = layout.computed.size.width;
  let h = layout.computed.size.height;
  // Rounded clip only applies when the whole box is clipped (both axes);
  // a single-axis clip has no meaningful corners to round.
  let clip_radius = match &element.kind {
    ElementKind::View(v) if clip_x && clip_y => v.clip_radius,
    _ => None,
  };
  if let Some([tl, tr, br, bl]) = clip_radius {
    let rect = Rect::new(IPoint::new(0.0, 0.0), ISize::new(w, h));
    let radii = RoundingRadii {
      top_left: IPoint::new(tl, tl),
      top_right: IPoint::new(tr, tr),
      bottom_right: IPoint::new(br, br),
      bottom_left: IPoint::new(bl, bl),
    };
    builder.clip_rounded_rect(&rect, &radii, ClipOperation::Intersect);
  } else {
    let x_min = if clip_x { 0.0 } else { -CLIP_INF };
    let y_min = if clip_y { 0.0 } else { -CLIP_INF };
    let x_max = if clip_x { w } else { CLIP_INF };
    let y_max = if clip_y { h } else { CLIP_INF };
    let rect = Rect::new(IPoint::new(x_min, y_min), ISize::new(x_max - x_min, y_max - y_min));
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
  let element = scene.node(node_id);
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
    BoundaryMode::Snapshot => snapshot_node(scene, node_id, ctx, builder),
  }
}

// Snapshot gate: the subtree is rasterized into a texture at the current
// display scale and composited as a single quad until something inside it
// changes, its layout size changes, or the display scale changes. Content
// painting outside the layout box is cropped (unlike a recording boundary);
// the crop happens in untransformed local space, since the boundary's own
// transform is hoisted out of the raster and applied to the quad instead.
fn snapshot_node<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
) {
  let element = scene.node(node_id);
  let size = element.layout.as_ref().map(|l| l.computed.size).unwrap_or(Size::ZERO);
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
  let src = Rect::new(IPoint::new(0.0, 0.0), ISize::new(width * scale, height * scale));
  let dst = Rect::new(IPoint::new(0.0, 0.0), ISize::new(width, height));

  {
    let cache = element.paint_cache.borrow();
    if let Some(PaintCache::Snapshot { texture, width: w, height: h, scale: s }) = &*cache {
      if *w == width && *h == height && *s == scale {
        ctx.snapshots_reused += 1;
        draw_with_transform(builder, own.as_ref(), |b| {
          b.draw_texture_rect(texture, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
        });
        return;
      }
    }
  }

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  record_node(scene, node_id, ctx, &mut sub, hoist);
  let Some(dl) = sub.build() else { return };

  match ctx.alloy.render_display_list_to_texture(&dl, tex_w, tex_h) {
    Ok(texture) => {
      ctx.snapshots_rasterized += 1;
      draw_with_transform(builder, own.as_ref(), |b| {
        b.draw_texture_rect(&texture, &src, &dst, TextureSampling::Linear, opacity_paint.as_ref());
      });
      *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot { texture, width, height, scale });
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
  // explicitly). Under Hoist::Full there is nothing to restore.
  let needs_save = record_clip || (matches!(&element.kind, ElementKind::View(_)) && hoist != Hoist::Full);
  if needs_save {
    builder.save();
  }

  if hoist == Hoist::None {
    element.build(ctx, builder);
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
    let bounds = Rect::new(IPoint::new(-CLIP_INF, -CLIP_INF), ISize::new(2.0 * CLIP_INF, 2.0 * CLIP_INF));
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

    let pos = child.layout.as_ref().map(|l| l.computed.location).unwrap_or(Point::ZERO);

    builder.translate(pos.x, pos.y);

    if child.has_layout() {
      let layout = &child.layout.as_ref().unwrap().computed;
      let pad_left = layout.padding.left;
      let pad_top = layout.padding.top;
      let pad_right = layout.padding.right;
      let pad_bottom = layout.padding.bottom;

      ctx.size.w = layout.size.width - pad_left - pad_right;
      ctx.size.h = layout.size.height - pad_top - pad_bottom;

      build_recursive(scene, child_id, ctx, builder);
    } else {
      if let Some(layout) = &element.layout {
        ctx.size.w = layout.computed.size.width;
        ctx.size.h = layout.computed.size.height;
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
