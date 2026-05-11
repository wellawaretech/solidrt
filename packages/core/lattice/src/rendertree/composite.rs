use alloy::impellers::DisplayListBuilder;
use taffy::prelude::*;
use taffy::Point;

use crate::rendertree::{WH, BuildContext, LayoutContext, ElementKind, RenderTree};

pub fn composite(
    builder: &mut DisplayListBuilder,
    tree: &mut RenderTree,
    root_id: u64,
) {
    let (width, height) = {
        let style = &tree.node(root_id).layout_data().style;
        (
            style.size.width.into_option().unwrap_or(800.0),
            style.size.height.into_option().unwrap_or(600.0),
        )
    };

    if tree.node(root_id).layout_data().cache.is_empty() {
        let available_space = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };
        let mut layout_ctx = LayoutContext { render_tree: tree };
        taffy::compute_root_layout(&mut layout_ctx, NodeId::from(root_id), available_space);
    }

    let mut ctx = BuildContext::new(&tree.typography_ctx);
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

    if let ElementKind::View(_) = &element.kind {
        builder.save();
    }

    element.build(ctx, builder);

    // Text children are Spans — not visual, skip recursion
    if let ElementKind::Text(_) = &element.kind {
        return;
    }

    for &child_id in &element.children {
        let child = scene.node(child_id);

        let pos = child
            .layout
            .as_ref()
            .map(|l| l.computed.location)
            .unwrap_or(Point::ZERO);

        ctx.origin.x += pos.x;
        ctx.origin.y += pos.y;
        builder.translate(pos.x, pos.y);

        if child.has_layout() {
            let layout = &child.layout.as_ref().unwrap().computed;
            let pad_left = layout.padding.left;
            let pad_top = layout.padding.top;
            let pad_right = layout.padding.right;
            let pad_bottom = layout.padding.bottom;

            ctx.size.w = layout.size.width - pad_left - pad_right;
            ctx.size.h = layout.size.height - pad_top - pad_bottom;
            ctx.origin.x += pad_left;
            ctx.origin.y += pad_top;

            build_recursive(scene, child_id, ctx, builder);

            ctx.origin.x -= pad_left;
            ctx.origin.y -= pad_top;
        } else {
            if let Some(layout) = &element.layout {
                ctx.size.w = layout.computed.size.width;
                ctx.size.h = layout.computed.size.height;
            }
            build_recursive(scene, child_id, ctx, builder);
        }

        ctx.origin.x -= pos.x;
        ctx.origin.y -= pos.y;
        builder.translate(-pos.x, -pos.y);
    }

    if let ElementKind::View(_) = &element.kind {
        builder.restore();
    }
}
