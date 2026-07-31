use taffy::style::Overflow;

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
  // `None` means "not explicitly set" - the element inherits its effective
  // value from the nearest ancestor that does set one (root default: Auto).
  // This mirrors CSS `pointer-events`, which is itself inherited: a `None` on
  // a container is expected to make everything under it non-hittable unless
  // a descendant opts back in explicitly, so a caller doesn't have to repeat
  // the same value on every leaf under a "click-through" overlay.
  pub pointer_events: Option<PointerEvents>,
}

impl Default for HitConfig {
  fn default() -> Self {
    Self { pointer_events: None }
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
      ElementKind::Texture(n) => n.is_in_bounds(point, ctx),
      ElementKind::Span(_) => false,
      _ => point.x >= 0.0 && point.x < ctx.size.w && point.y >= 0.0 && point.y < ctx.size.h,
    }
  }
}

/// (node_id, parent-space point, local point after element's transform)
pub type HitEntry = (u64, XY, XY);

/// Diffs a previously-hovered hit path against a freshly computed one, both
/// ordered root->leaf, relative to their longest shared prefix. Returns:
/// - `left`: ids no longer in the path, deepest-first (leaf->root), so a
///   consumer dispatches leave events from the innermost element outward.
/// - `entered`: newly present ids, root->leaf, for enter events outermost-in.
pub fn path_diff(old_ids: &[u64], new_ids: &[u64]) -> (Vec<u64>, Vec<u64>) {
  let mut diverge = 0;
  while diverge < old_ids.len() && diverge < new_ids.len() && old_ids[diverge] == new_ids[diverge] {
    diverge += 1;
  }
  let left: Vec<u64> = old_ids[diverge..].iter().rev().copied().collect();
  let entered: Vec<u64> = new_ids[diverge..].to_vec();
  (left, entered)
}

/// Project a window-space point into the local frame of every node along a
/// root->leaf id chain, replaying `hit_recursive`'s descent math without the
/// bounds checks: the chain does not need to be under the pointer, so a stored
/// path (frozen-drag routing, hover diffs) still yields exact locals. A node
/// missing from the tree truncates the result - the frames below a dead node
/// are meaningless.
pub fn locals_along_path(tree: &RenderTree, chain: &[u64], point: XY) -> Vec<XY> {
  let mut locals = Vec::with_capacity(chain.len());
  let mut point = point;
  let mut parent_size = WH::default();
  let mut parent_scroll = XY::default();
  for (i, &id) in chain.iter().enumerate() {
    let Some(element) = tree.try_node(id) else { break };
    let size = element
      .layout
      .as_ref()
      .map(|l| WH::new(l.computed.size.width, l.computed.size.height))
      .unwrap_or(parent_size);
    if i > 0 {
      let pos = element
        .layout
        .as_ref()
        .map(|l| XY::new(l.computed.location.x, l.computed.location.y))
        .unwrap_or_default();
      point = XY::new(point.x - pos.x + parent_scroll.x, point.y - pos.y + parent_scroll.y);
    }
    let local = element.kind.transform_to_local(point, &HitContext { size });
    locals.push(local);
    point = local;
    // A viewBox view hands its children the design-space size, matching
    // hit_recursive and the paint-time walk.
    parent_size = match &element.kind {
      ElementKind::View(v) => v.view_box.unwrap_or(size),
      _ => size,
    };
    parent_scroll = match &element.kind {
      ElementKind::View(v) => v.scroll.unwrap_or_default(),
      _ => XY::default(),
    };
  }
  locals
}

pub trait HitTester {
  fn hit_test(&self, tree: &RenderTree, point: XY) -> Vec<HitEntry>;
}

pub struct DefaultHitTester;

impl HitTester for DefaultHitTester {
  fn hit_test(&self, tree: &RenderTree, point: XY) -> Vec<HitEntry> {
    let Some(root_id) = tree.root else {
      return vec![];
    };
    let size = tree
      .node(root_id)
      .layout
      .as_ref()
      .map(|l| WH::new(l.computed.size.width, l.computed.size.height))
      .unwrap_or_default();
    let mut path = Vec::new();
    hit_recursive(tree, root_id, point, size, PointerEvents::Auto, &mut path);
    path
  }
}

fn hit_recursive(
  tree: &RenderTree,
  node_id: u64,
  point: XY,
  size: WH,
  inherited: PointerEvents,
  path: &mut Vec<HitEntry>,
) -> bool {
  let element = tree.node(node_id);

  // An explicit local value wins; otherwise the resolved value cascades down
  // from the parent (see the comment on HitConfig::pointer_events).
  let pointer_events = element.interaction.as_ref().and_then(|i| i.pointer_events).unwrap_or(inherited);

  let ctx = HitContext { size };
  let local = element.kind.transform_to_local(point, &ctx);

  // Overflow gate: when an axis has non-visible overflow, the layout box clips
  // both self and any descendants on that axis. Mirrors the paint-time clip in
  // composite.rs.
  let (overflow_x, overflow_y) = element
    .layout
    .as_ref()
    .map(|l| (l.style.overflow.x, l.style.overflow.y))
    .unwrap_or((Overflow::Visible, Overflow::Visible));
  let clipped_out = (overflow_x != Overflow::Visible && (local.x < 0.0 || local.x >= size.w))
    || (overflow_y != Overflow::Visible && (local.y < 0.0 || local.y >= size.h));
  if clipped_out {
    return false;
  }

  if pointer_events == PointerEvents::Auto && !element.kind.is_in_bounds(local, &ctx) {
    return false;
  }

  let my_index = path.len();
  path.push((node_id, point, local));

  if pointer_events == PointerEvents::All && element.kind.is_in_bounds(local, &ctx) {
    return true;
  }

  // Scroll offset on a View shifts its children's apparent positions by -scroll
  // in viewport space, so to map a viewport-local point into a child's frame we
  // add scroll back.
  let scroll = match &element.kind {
    ElementKind::View(v) => v.scroll.unwrap_or_default(),
    _ => XY::default(),
  };

  // Children of a viewBox view live in the design space; the local point is
  // already there (the inverse matrix includes the fit), so their inherited
  // box is the design size, matching the paint-time walk in composite.rs.
  let inherited_size = match &element.kind {
    ElementKind::View(v) => v.view_box.unwrap_or(size),
    _ => size,
  };

  for &child_id in element.children.iter().rev() {
    let child = tree.node(child_id);
    let child_size =
      child.layout.as_ref().map(|l| WH::new(l.computed.size.width, l.computed.size.height)).unwrap_or(inherited_size);
    let child_pos =
      child.layout.as_ref().map(|l| XY::new(l.computed.location.x, l.computed.location.y)).unwrap_or_default();
    let child_point = XY::new(local.x - child_pos.x + scroll.x, local.y - child_pos.y + scroll.y);
    if hit_recursive(tree, child_id, child_point, child_size, pointer_events, path) {
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
