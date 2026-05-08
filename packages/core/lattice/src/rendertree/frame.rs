use alloy::impellers::{DisplayListBuilder, TypographyContext};
use taffy::prelude::*;
use taffy::Point;

use crate::rendertree::WH;
use crate::rendertree::{BuildContext, LayoutContext, Primitive, RenderTree};

pub fn composite(
    builder: &mut DisplayListBuilder,
    scene: &mut RenderTree,
    root_id: NodeId,
    typography_ctx: &TypographyContext,
) {
    let mut ctx = BuildContext::new(typography_ctx);

    let root_style = &scene.node(root_id).layout_data().style;
    let width = root_style.size.width.into_option().unwrap_or(800.0);
    let height = root_style.size.height.into_option().unwrap_or(600.0);
    ctx.size = WH::new(width, height);

    if scene.node(root_id).layout_data().cache.is_empty() {
        let available_space = Size {
            width: AvailableSpace::Definite(width),
            height: AvailableSpace::Definite(height),
        };
        let mut layout_ctx = LayoutContext { render_tree: scene, typography_ctx };
        taffy::compute_root_layout(&mut layout_ctx, root_id, available_space);
    }

    build_recursive(scene, root_id, &mut ctx, builder);
}

fn build_recursive<'a>(
    scene: &'a RenderTree,
    node_id: NodeId,
    ctx: &mut BuildContext<'a>,
    builder: &mut DisplayListBuilder,
) {
    let node = scene.node(node_id);

    if let Primitive::View(_) = &node.node_type {
        builder.save();
    }

    node.build(ctx, builder);

    // TextNode children are StringNodes — not visual, skip recursion
    if let Primitive::Text(_) = &node.node_type {
        return;
    }

    for &child_id in &node.children {
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
            if let Some(layout) = &node.layout {
                ctx.size.w = layout.computed.size.width;
                ctx.size.h = layout.computed.size.height;
            }
            build_recursive(scene, child_id, ctx, builder);
        }

        ctx.origin.x -= pos.x;
        ctx.origin.y -= pos.y;
        builder.translate(-pos.x, -pos.y);
    }

    if let Primitive::View(_) = &node.node_type {
        builder.restore();
    }
}
