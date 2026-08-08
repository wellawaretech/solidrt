use taffy::style::Overflow;

use super::{ElementKind, Point, RenderTree, Size, Vector};

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

/// Which routed pointer deliveries an element wants, as a bitmask: the
/// consumer's per-node handler registry mirrored into the tree, so the router
/// can skip building deliveries that would reach nobody (see router.rs).
/// Per-node presence, not inherited - a handler either exists on a node or it
/// does not. Down and Up are recorded but never gated (focus and gesture side
/// effects hang off them regardless of handlers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventInterest(pub u32);

impl EventInterest {
  pub const MOVE: u32 = 1 << 0;
  pub const DOWN: u32 = 1 << 1;
  pub const UP: u32 = 1 << 2;
  pub const ENTER: u32 = 1 << 3;
  pub const LEAVE: u32 = 1 << 4;
  pub const WHEEL: u32 = 1 << 5;
  /// Every bit with a defined meaning; decode sites reject anything outside.
  pub const KNOWN: u32 = Self::MOVE | Self::DOWN | Self::UP | Self::ENTER | Self::LEAVE | Self::WHEEL;

  pub fn has(self, bit: u32) -> bool {
    self.0 & bit != 0
  }
}

pub struct HitConfig {
  // `None` means "not explicitly set" - the element inherits its effective
  // value from the nearest ancestor that does set one (root default: Auto).
  // This mirrors CSS `pointer-events`, which is itself inherited: a `None` on
  // a container is expected to make everything under it non-hittable unless
  // a descendant opts back in explicitly, so a caller doesn't have to repeat
  // the same value on every leaf under a "click-through" overlay.
  pub pointer_events: Option<PointerEvents>,
  pub listens: EventInterest,
}

impl Default for HitConfig {
  fn default() -> Self {
    Self { pointer_events: None, listens: EventInterest::default() }
  }
}

pub struct HitContext {
  pub size: Size,
}

pub trait Hittable {
  fn transform_to_local(&self, point: Point, _ctx: &HitContext) -> Point {
    point
  }

  fn is_in_bounds(&self, point: Point, ctx: &HitContext) -> bool {
    point.x >= 0.0 && point.x < ctx.size.width && point.y >= 0.0 && point.y < ctx.size.height
  }
}

impl Hittable for ElementKind {
  fn transform_to_local(&self, point: Point, ctx: &HitContext) -> Point {
    match self {
      ElementKind::View(n) => n.transform_to_local(point, ctx),
      _ => point,
    }
  }

  fn is_in_bounds(&self, point: Point, ctx: &HitContext) -> bool {
    match self {
      ElementKind::Rectangle(n) => n.is_in_bounds(point, ctx),
      // ElementKind::Oval(n) => n.is_in_bounds(point, ctx),
      ElementKind::Path(n) => n.is_in_bounds(point, ctx),
      ElementKind::Texture(n) => n.is_in_bounds(point, ctx),
      ElementKind::Span(_) => false,
      _ => point.x >= 0.0 && point.x < ctx.size.width && point.y >= 0.0 && point.y < ctx.size.height,
    }
  }
}

/// (node_id, parent-space point, local point after element's transform)
pub type HitEntry = (u64, Point, Point);

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
pub fn locals_along_path(tree: &RenderTree, chain: &[u64], point: Point) -> Vec<Point> {
  let mut locals = Vec::with_capacity(chain.len());
  let mut point = point;
  let mut parent_size = Size::default();
  let mut parent_scroll = Vector::default();
  for (i, &id) in chain.iter().enumerate() {
    let Some(element) = tree.try_node(id) else { break };
    let size = element.layout.as_ref().map(|l| l.size()).unwrap_or(parent_size);
    if i > 0 {
      let pos = element.layout.as_ref().map(|l| l.location()).unwrap_or_default();
      point = point - pos.to_vector() + parent_scroll;
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
      _ => Vector::default(),
    };
  }
  locals
}

pub trait HitTester {
  fn hit_test(&self, tree: &RenderTree, point: Point) -> Vec<HitEntry>;
}

pub struct DefaultHitTester;

impl HitTester for DefaultHitTester {
  fn hit_test(&self, tree: &RenderTree, point: Point) -> Vec<HitEntry> {
    let Some(root_id) = tree.root else {
      return vec![];
    };
    let size = tree.node(root_id).layout.as_ref().map(|l| l.size()).unwrap_or_default();
    let mut path = Vec::new();
    hit_recursive(tree, root_id, point, size, PointerEvents::Auto, &mut path);
    path
  }
}

fn hit_recursive(
  tree: &RenderTree,
  node_id: u64,
  point: Point,
  size: Size,
  inherited: PointerEvents,
  path: &mut Vec<HitEntry>,
) -> bool {
  let element = tree.node(node_id);

  // An explicit local value wins; otherwise the resolved value cascades down
  // from the parent (see the comment on HitConfig::pointer_events).
  let pointer_events = element.interaction.as_ref().and_then(|i| i.pointer_events).unwrap_or(inherited);

  let ctx = HitContext { size };
  let local = element.kind.transform_to_local(point, &ctx);

  // `local` lives in the frame the element's transform maps INTO, which for a
  // viewBox view is the design space, not the layout box (the inverse includes
  // the fit). Bounds have to be measured in that same frame: against the box a
  // design space wider than its box would reject its own overflowing part, and
  // a rejected view takes its whole subtree with it.
  let local_size = match &element.kind {
    ElementKind::View(v) => v.view_box.unwrap_or(size),
    _ => size,
  };
  let local_ctx = HitContext { size: local_size };

  // Overflow gate: when an axis has non-visible overflow, the layout box clips
  // both self and any descendants on that axis. The clip means the BOX, so the
  // gate measures in box space: on a viewBox view `local` is design-space and
  // is mapped forward through the fit first - mirroring record_node, which
  // emits the clip under the user chain before the fit
  // (okf/backlog/overflow-viewbox-clip.md).
  let (overflow_x, overflow_y) = element
    .layout
    .as_ref()
    .map(|l| (l.style.overflow.x, l.style.overflow.y))
    .unwrap_or((Overflow::Visible, Overflow::Visible));
  let box_local = match &element.kind {
    ElementKind::View(v) => match v.fit_matrix(size) {
      // The fit is affine (uniform scale + translate), so transform_point2d
      // cannot fail on it.
      Some(fit) => fit.transform_point2d(local).unwrap_or(local),
      None => local,
    },
    _ => local,
  };
  let clipped_out = (overflow_x != Overflow::Visible && (box_local.x < 0.0 || box_local.x >= size.width))
    || (overflow_y != Overflow::Visible && (box_local.y < 0.0 || box_local.y >= size.height));
  if clipped_out {
    return false;
  }

  if pointer_events == PointerEvents::Auto && !element.kind.is_in_bounds(local, &local_ctx) {
    return false;
  }

  let my_index = path.len();
  path.push((node_id, point, local));

  if pointer_events == PointerEvents::All && element.kind.is_in_bounds(local, &local_ctx) {
    return true;
  }

  // Scroll offset on a View shifts its children's apparent positions by -scroll
  // in viewport space, so to map a viewport-local point into a child's frame we
  // add scroll back.
  let scroll = match &element.kind {
    ElementKind::View(v) => v.scroll.unwrap_or_default(),
    _ => Vector::default(),
  };

  // Children inherit the frame `local` is in - the design size under a viewBox
  // view, matching the paint-time walk in composite.rs.
  for &child_id in element.children.iter().rev() {
    let child = tree.node(child_id);
    let child_size = child.layout.as_ref().map(|l| l.size()).unwrap_or(local_size);
    let child_pos = child.layout.as_ref().map(|l| l.location()).unwrap_or_default();
    let child_point = local - child_pos.to_vector() + scroll;
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
