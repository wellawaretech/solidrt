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
      ElementKind::Path(n) => n.is_in_bounds(point, ctx),
      // ElementKind::Texture(n) => n.is_in_bounds(point, ctx),
      ElementKind::Span(_) => false,
      _ => point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h,
    }
  }
}

/// (node_id, parent-space point, local point after element's transform)
pub type HitEntry = (u64, XY, XY);

pub trait HitTester {
  fn hit_test(&self, tree: &RenderTree, point: XY) -> Vec<HitEntry>;
}

pub struct DefaultHitTester;

impl HitTester for DefaultHitTester {
  fn hit_test(&self, tree: &RenderTree, point: XY) -> Vec<HitEntry> {
    let Some(root_id) = tree.root else { return vec![] };
    let size = tree
      .node(root_id)
      .layout
      .as_ref()
      .map(|l| WH::new(l.computed.size.width, l.computed.size.height))
      .unwrap_or_default();
    let mut path = Vec::new();
    hit_recursive(tree, root_id, point, size, &mut path);
    path
  }
}

fn hit_recursive(tree: &RenderTree, node_id: u64, point: XY, size: WH, path: &mut Vec<HitEntry>) -> bool {
  let element = tree.node(node_id);

  let pointer_events = element.interaction.as_ref()
    .map(|i| i.pointer_events)
    .unwrap_or(PointerEvents::Auto);

  let ctx = HitContext { size };
  let local = element.kind.transform_to_local(point, &ctx);

  if pointer_events == PointerEvents::Auto && !element.kind.is_in_bounds(local, &ctx) {
    return false;
  }

  let my_index = path.len();
  path.push((node_id, point, local));

  if pointer_events == PointerEvents::All && element.kind.is_in_bounds(local, &ctx) {
    return true;
  }

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
    if hit_recursive(tree, child_id, child_point, child_size, path) {
      if pointer_events == PointerEvents::None {
        path.remove(my_index);
      }
      return true;
    }
  }

  if pointer_events == PointerEvents::None {
    path.pop();
    return false;
  }

  true
}
