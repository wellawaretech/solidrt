use alloy::impellers::{ClipOperation, DisplayListBuilder, Point as IPoint, Rect, Size as ISize};
use taffy::prelude::*;
use taffy::style::Overflow;
use taffy::Point;

use crate::rendertree::{
  BuildContext, ElementKind, LayoutContext, PlatformContext, RenderTree, WH,
};

// Large finite extent used to leave one axis effectively unclipped when only the
// other axis has non-visible overflow. clip_rect requires a finite rectangle.
const CLIP_INF: f32 = 1.0e7;

// Runs taffy layout. Safe to call repeatedly: taffy's per-node cache makes
// a second call cheap when nothing has been invalidated since the previous run.
pub fn layout_phase(tree: &mut RenderTree, platform: &PlatformContext) {
  let Some(root_id) = tree.root else { return };
  let (width, height) = platform.window_size();

  if platform.take_window_size_dirty() {
    tree.invalidate_cache(root_id);
  }

  let available_space = Size {
    width: AvailableSpace::Definite(width),
    height: AvailableSpace::Definite(height),
  };
  let mut layout_ctx = LayoutContext {
    render_tree: tree,
    platform,
  };
  taffy::compute_root_layout(&mut layout_ctx, NodeId::from(root_id), available_space);
}

// Picks up any cache invalidations queued by onLayout handlers, then paints.
pub fn paint_phase(
  builder: &mut DisplayListBuilder,
  tree: &mut RenderTree,
  platform: &PlatformContext,
) {
  let Some(root_id) = tree.root else { return };
  let (width, height) = platform.window_size();

  layout_phase(tree, platform);

  let mut ctx = BuildContext::new(platform);
  ctx.size = WH::new(width, height);
  build_recursive(tree, root_id, &mut ctx, builder);
}

fn build_recursive<'a>(
  scene: &'a RenderTree,
  node_id: u64,
  ctx: &mut BuildContext<'a>,
  builder: &mut DisplayListBuilder,
) {
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
      let x_min = if clip_x { 0.0 } else { -CLIP_INF };
      let y_min = if clip_y { 0.0 } else { -CLIP_INF };
      let x_max = if clip_x { w } else { CLIP_INF };
      let y_max = if clip_y { h } else { CLIP_INF };
      let rect = Rect::new(
        IPoint::new(x_min, y_min),
        ISize::new(x_max - x_min, y_max - y_min),
      );
      builder.clip_rect(&rect, ClipOperation::Intersect);
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

    let pos = child
      .layout
      .as_ref()
      .map(|l| l.computed.location)
      .unwrap_or(Point::ZERO);

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