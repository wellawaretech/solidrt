use super::{ElementKind, RenderTree, WH, XY};

/// Controls whether an element participates in hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEvents {
    /// Default: element is hit-testable; miss clips children.
    Auto,
    /// Element is transparent to hit testing.
    None,
    /// Element captures all pointer events within bounds, stopping propagation.
    All,
}

pub struct HitConfig {
    pub pointer_events: PointerEvents,
}

impl Default for HitConfig {
    fn default() -> Self {
        Self {
            pointer_events: PointerEvents::Auto,
        }
    }
}

pub struct HitContext {
    pub size: WH,
}

pub trait Hittable {
    fn transform_to_local(&self, point: XY, _ctx: &HitContext) -> XY {
        point
    }

    fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
        point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h
    }
}

impl Hittable for ElementKind {
    fn transform_to_local(&self, point: XY, ctx: &HitContext) -> XY {
        match self {
            ElementKind::View(n) => n.transform_to_local(point, ctx),
            _ => point,
        }
    }

    fn is_in_bounds(&self, point: XY, ctx: &HitContext) -> bool {
        match self {
            ElementKind::Rectangle(n) => n.is_in_bounds(point, ctx),
            // ElementKind::Oval(n) => n.is_in_bounds(point, ctx),
            // ElementKind::Path(n) => n.is_in_bounds(point, ctx),
            // ElementKind::Texture(n) => n.is_in_bounds(point, ctx),
            ElementKind::Span(_) => false,
            _ => point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h,
        }
    }
}

pub trait HitTester {
    fn hit_test(&self, tree: &RenderTree, point: XY) -> Option<u64>;
}

pub struct DefaultHitTester;

impl HitTester for DefaultHitTester {
    fn hit_test(&self, tree: &RenderTree, point: XY) -> Option<u64> {
        let root_id = tree.root?;
        let size = tree
            .node(root_id)
            .layout
            .as_ref()
            .map(|l| WH::new(l.computed.size.width, l.computed.size.height))
            .unwrap_or_default();
        hit_recursive(tree, root_id, point, size)
    }
}

fn hit_recursive(tree: &RenderTree, node_id: u64, point: XY, size: WH) -> Option<u64> {
    let element = tree.node(node_id);

    if let Some(input) = &element.interaction {
        if input.pointer_events == PointerEvents::None {
            return None;
        }
    }

    let ctx = HitContext { size };
    let local = element.kind.transform_to_local(point, &ctx);

    for &child_id in element.children.iter().rev() {
        let child = tree.node(child_id);
        let child_size = child
            .layout
            .as_ref()
            .map(|l| WH::new(l.computed.size.width, l.computed.size.height))
            .unwrap_or(size);
        let child_pos = child
            .layout
            .as_ref()
            .map(|l| XY::new(l.computed.location.x, l.computed.location.y))
            .unwrap_or_default();
        let child_point = XY::new(local.x - child_pos.x, local.y - child_pos.y);
        if let Some(hit) = hit_recursive(tree, child_id, child_point, child_size) {
            return Some(hit);
        }
    }

    if element.interaction.is_some() && element.kind.is_in_bounds(local, &ctx) {
        return Some(node_id);
    }

    None
}
