use crate::impellers::{
  ClipOperation, DisplayListBuilder, Point as IPoint, Rect, RoundingRadii, Size as ISize, TextureSampling,
};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::Point;

use crate::rendertree::{
  BoundaryMode, BuildContext, ElementKind, LayoutContext, PaintCache, PlatformContext, RenderTree, WH,
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

// Repaint-boundary gate: a boundary subtree's paint result is retained (as a
// recording or as rasterized pixels, in node-local coordinates; the parent has
// already translated the builder) and replayed from the cache until something
// inside it changes (see RenderTree::invalidate_paint).
fn build_recursive<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
) {
  let element = scene.node(node_id);
  match element.repaint_boundary {
    BoundaryMode::None => record_node(scene, node_id, ctx, builder),
    BoundaryMode::Recording => {
      let cached = match &*element.paint_cache.borrow() {
        Some(PaintCache::Recording(dl)) => Some(dl.clone()),
        _ => None,
      };
      if let Some(dl) = cached {
        ctx.boundaries_reused += 1;
        builder.draw_display_list(&dl, 1.0);
        return;
      }
      let mut sub = DisplayListBuilder::new(None);
      record_node(scene, node_id, ctx, &mut sub);
      if let Some(dl) = sub.build() {
        ctx.boundaries_recorded += 1;
        builder.draw_display_list(&dl, 1.0);
        *element.paint_cache.borrow_mut() = Some(PaintCache::Recording(dl));
      }
    }
    BoundaryMode::Snapshot => snapshot_node(scene, node_id, ctx, builder),
  }
}

// Snapshot gate: the subtree is rasterized into a texture at the current
// display scale and composited as a single quad until something inside it
// changes, its layout size changes, or the display scale changes. Content
// painting outside the layout box is cropped (unlike a recording boundary).
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
    record_node(scene, node_id, ctx, builder);
    return;
  }

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
        builder.draw_texture_rect(texture, &src, &dst, TextureSampling::Linear, None);
        return;
      }
    }
  }

  let mut sub = DisplayListBuilder::new(None);
  sub.scale(scale, scale);
  record_node(scene, node_id, ctx, &mut sub);
  let Some(dl) = sub.build() else { return };

  match ctx.alloy.render_display_list_to_texture(&dl, tex_w, tex_h) {
    Ok(texture) => {
      ctx.snapshots_rasterized += 1;
      builder.draw_texture_rect(&texture, &src, &dst, TextureSampling::Linear, None);
      *element.paint_cache.borrow_mut() = Some(PaintCache::Snapshot { texture, width, height, scale });
    }
    Err(e) => {
      // Paint inline this frame; the recording carries its own device-scale
      // transform, so counter the enclosing CTM's scale before replaying.
      log::warn!("snapshot rasterization failed for node {node_id}: {e}; painting inline");
      builder.save();
      builder.scale(1.0 / scale, 1.0 / scale);
      builder.draw_display_list(&dl, 1.0);
      builder.restore();
    }
  }
}

fn record_node<'a>(scene: &'a RenderTree, node_id: u64, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
  let element = scene.node(node_id);

  let (overflow_x, overflow_y) = element
    .layout
    .as_ref()
    .map(|l| (l.style.overflow.x, l.style.overflow.y))
    .unwrap_or((Overflow::Visible, Overflow::Visible));
  let clip_x = overflow_x != Overflow::Visible;
  let clip_y = overflow_y != Overflow::Visible;
  let needs_clip = clip_x || clip_y;

  let needs_save = matches!(&element.kind, ElementKind::View(_)) || needs_clip;
  if needs_save {
    builder.save();
  }

  element.build(ctx, builder);

  if needs_clip {
    if let Some(layout) = &element.layout {
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
  }

  // Scroll offset: applied after the clip so the clip box stays put in
  // viewport space while children slide under it. Positive scroll shifts
  // content leftward/upward.
  if let ElementKind::View(view) = &element.kind {
    if let Some(s) = view.scroll {
      builder.translate(-s.x, -s.y);
    }
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

  if needs_save {
    builder.restore();
  }
}
