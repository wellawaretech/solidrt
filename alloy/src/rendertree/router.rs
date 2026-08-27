use std::collections::HashMap;

use super::hit::{locals_along_path, path_diff, DefaultHitTester, EventInterest, HitEntry, HitTester};
use super::{Point, RenderTree};
use crate::{Modifiers, PointerType};

pub type PointerKey = (PointerType, u64);

pub enum InputEvent {
  // `dx`/`dy` is the movement since the previous move: the resampler's
  // resolved delta (summed hardware deltas for mouse, dispatched-position
  // diff otherwise), in the same logical units as `x`/`y`.
  PointerMove { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, dx: f32, dy: f32, modifiers: Modifiers },
  PointerDown { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  PointerUp { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  Wheel { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, delta_x: f32, delta_y: f32, modifiers: Modifiers },
}

/// The kind of a routed delivery, carrying the per-kind payload.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RoutedKind {
  Move { dx: f32, dy: f32 },
  Down { button: u8 },
  Up { button: u8 },
  Enter,
  Leave,
  Wheel { delta_x: f32, delta_y: f32 },
}

/// One event delivery for a consumer to dispatch along `targets`. The arrays
/// are index-parallel: the pointer position in each target's own coordinate
/// frame (`locals`) and in its path-parent's frame (`parents`, the window
/// frame for the root). Targets are ordered root->leaf, except Leave, which
/// is deepest-first (leave dispatches innermost-out). `target` is the deepest
/// node of the full path the event was computed from (also for enter/leave
/// subsets).
pub struct RoutedPointer {
  pub kind: RoutedKind,
  pub pointer_id: u64,
  pub pointer_type: PointerType,
  /// Window-space pointer position.
  pub position: Point,
  pub modifiers: Modifiers,
  pub targets: Vec<u64>,
  pub locals: Vec<Point>,
  pub parents: Vec<Point>,
  pub target: u64,
}

fn delivery(
  kind: RoutedKind,
  key: PointerKey,
  point: Point,
  modifiers: Modifiers,
  targets: Vec<u64>,
  locals: Vec<Point>,
  parents: Vec<Point>,
  target: u64,
) -> RoutedPointer {
  debug_assert_eq!(targets.len(), locals.len(), "targets and locals must stay parallel");
  debug_assert_eq!(targets.len(), parents.len(), "targets and parents must stay parallel");
  let (pointer_type, pointer_id) = key;
  RoutedPointer { kind, pointer_id, pointer_type, position: point, modifiers, targets, locals, parents, target }
}

fn split_path(path: Vec<HitEntry>) -> (Vec<u64>, Vec<Point>) {
  path.into_iter().map(|(id, _, local)| (id, local)).unzip()
}

// Parent-frame points for a full root->leaf chain: the window point for the
// root, each node's local for its child.
fn parent_locals(point: Point, locals: &[Point]) -> Vec<Point> {
  std::iter::once(point).chain(locals.iter().copied()).take(locals.len()).collect()
}

// Targets + exact locals for an event routed along a stored chain: project the
// point along it; ids past a truncated projection (their nodes were removed
// mid-gesture) are dropped so the arrays stay parallel.
fn routed(tree: &RenderTree, chain: Vec<u64>, point: Point) -> (Vec<u64>, Vec<Point>) {
  let locals = locals_along_path(tree, &chain, point);
  let mut ids = chain;
  ids.truncate(locals.len());
  (ids, locals)
}

// True when any node along `ids` wants `bit` deliveries (see
// EventInterest): bubbling delivers to every node on the path, so one
// listener anywhere keeps the delivery alive. Gating never changes who a
// delivery goes to - only whether one that would reach nobody is built at
// all. During a drag this runs over the frozen down-path, so a container
// listening for moves keeps a gesture flowing while the pointer is over
// nothing (the titlebar-drag case).
fn wants(tree: &RenderTree, ids: &[u64], bit: u32) -> bool {
  ids.iter().any(|&id| tree.try_node(id).is_some_and(|el| el.interaction.as_ref().is_some_and(|i| i.listens.has(bit))))
}

// Locals and parent-frame points for `subset` of `chain` (a path_diff result
// or a reversed leave list), by projecting along the full chain and picking
// entries out. An id the projection could not reach (its node was removed)
// keeps the window point; nothing can observe it - a removed node has no
// handlers.
fn pick_locals(tree: &RenderTree, chain: &[u64], subset: &[u64], point: Point) -> (Vec<Point>, Vec<Point>) {
  let chain_locals = locals_along_path(tree, chain, point);
  subset
    .iter()
    .map(|id| match chain.iter().position(|c| c == id) {
      Some(i) => {
        let local = chain_locals.get(i).copied().unwrap_or(point);
        let parent = if i == 0 { point } else { chain_locals.get(i - 1).copied().unwrap_or(point) };
        (local, parent)
      }
      None => (point, point),
    })
    .unzip()
}

/// Per-pointer path state aimed at this render tree: the hovered path
/// (enter/leave diffing) and the path frozen at pointerDown. While a pointer
/// has an active down, its moves and up route along the frozen path no matter
/// where the pointer currently is, so every node under the original down
/// observes the whole gesture (drags keep working off-element, ancestor
/// recognizers see the moves they need). Freezing lives here rather than in
/// the consumer so exact local coordinates can be projected along the frozen
/// chain. The stored paths hold node ids into the tree being routed for; a
/// router must not outlive its tree's id space (a rebuilt tree gets a fresh
/// router).
#[derive(Default)]
pub struct PointerRouter {
  hovered: HashMap<PointerKey, Vec<u64>>,
  down: HashMap<PointerKey, Vec<u64>>,
}

impl PointerRouter {
  /// Route one input event against the tree's last computed layout, returning
  /// the deliveries to dispatch, in order. Every projection resolves here,
  /// against the one tree state the hit test saw; handlers a consumer runs
  /// between deliveries cannot skew the geometry of later ones.
  ///
  /// Move, Wheel, Enter and Leave deliveries that would reach no listener
  /// (per the path's EventInterest bits) are not built. Down and Up always
  /// deliver - consumer side effects (focus, gestures) hang off them
  /// regardless of handlers - and hover state updates whether or not its
  /// enter/leave deliveries are gated.
  pub fn dispatch(&mut self, tree: &RenderTree, event: InputEvent) -> Vec<RoutedPointer> {
    match event {
      InputEvent::PointerMove { pointer_id, pointer_type, x, y, dx, dy, modifiers } => {
        let key = (pointer_type, pointer_id);
        let point = Point::new(x, y);
        let (live_ids, live_locals) = split_path(DefaultHitTester.hit_test(tree, point));
        let (ids, locals) = match self.down.get(&key) {
          Some(frozen) => routed(tree, frozen.clone(), point),
          None => (live_ids.clone(), live_locals),
        };
        let mut events = Vec::new();
        if wants(tree, &ids, EventInterest::MOVE) {
          let parents = parent_locals(point, &locals);
          let target = ids.last().copied().unwrap_or(0);
          events.push(delivery(RoutedKind::Move { dx, dy }, key, point, modifiers, ids, locals, parents, target));
        }
        events.extend(self.update_hover(tree, key, point, modifiers, live_ids));
        events
      }
      InputEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
        let key = (pointer_type, pointer_id);
        let point = Point::new(x, y);
        let (ids, locals) = split_path(DefaultHitTester.hit_test(tree, point));
        if ids.is_empty() {
          return vec![];
        }
        self.down.insert(key, ids.clone());
        let parents = parent_locals(point, &locals);
        let target = ids.last().copied().unwrap_or(0);
        vec![delivery(RoutedKind::Down { button }, key, point, modifiers, ids, locals, parents, target)]
      }
      InputEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
        let key = (pointer_type, pointer_id);
        let point = Point::new(x, y);
        let (ids, locals) = match self.down.remove(&key) {
          Some(frozen) => routed(tree, frozen, point),
          None => split_path(DefaultHitTester.hit_test(tree, point)),
        };
        let parents = parent_locals(point, &locals);
        let target = ids.last().copied().unwrap_or(0);
        let mut events = vec![delivery(RoutedKind::Up { button }, key, point, modifiers, ids, locals, parents, target)];

        // For touch, the pointer ends here. Deliver a final Leave for
        // anything still in its hovered path so the consumer can clean up,
        // and drop the hover entry to prevent it from leaking across future
        // touches.
        if pointer_type == PointerType::Touch {
          let old_ids = self.hovered.remove(&key).unwrap_or_default();
          if wants(tree, &old_ids, EventInterest::LEAVE) {
            let leave: Vec<u64> = old_ids.iter().rev().copied().collect();
            let (leave_locals, leave_parents) = pick_locals(tree, &old_ids, &leave, point);
            let target = old_ids.last().copied().unwrap_or(0);
            events.push(delivery(RoutedKind::Leave, key, point, modifiers, leave, leave_locals, leave_parents, target));
          }
        }
        events
      }
      InputEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
        let key = (pointer_type, pointer_id);
        let point = Point::new(x, y);
        let (ids, locals) = split_path(DefaultHitTester.hit_test(tree, point));
        if !wants(tree, &ids, EventInterest::WHEEL) {
          return vec![];
        }
        let parents = parent_locals(point, &locals);
        let target = ids.last().copied().unwrap_or(0);
        vec![delivery(RoutedKind::Wheel { delta_x, delta_y }, key, point, modifiers, ids, locals, parents, target)]
      }
    }
  }

  /// Re-run the hover diff for every live pointer, e.g. after each produced
  /// frame: layout changes can move elements under a stationary cursor, which
  /// arrival-time dispatch cannot see.
  pub fn refresh_hover(
    &mut self,
    tree: &RenderTree,
    pointers: Vec<(PointerKey, (f32, f32))>,
    modifiers: Modifiers,
  ) -> Vec<RoutedPointer> {
    let mut events = Vec::new();
    for (key, (px, py)) in pointers {
      let point = Point::new(px, py);
      let path = DefaultHitTester.hit_test(tree, point);
      let new_ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
      events.extend(self.update_hover(tree, key, point, modifiers, new_ids));
    }
    events
  }

  // Leave (deepest-first) and Enter (outermost-in) deliveries for the
  // hovered-path delta, storing the new path.
  fn update_hover(
    &mut self,
    tree: &RenderTree,
    key: PointerKey,
    point: Point,
    modifiers: Modifiers,
    new_ids: Vec<u64>,
  ) -> Vec<RoutedPointer> {
    let old_ids = self.hovered.get(&key).cloned().unwrap_or_default();
    let mut events = Vec::new();
    if new_ids != old_ids {
      let (left, entered) = path_diff(&old_ids, &new_ids);
      // Both projections resolve before either delivery is built, against the
      // same tree state the diff was computed from.
      let (left_locals, left_parents) = pick_locals(tree, &old_ids, &left, point);
      let (entered_locals, entered_parents) = pick_locals(tree, &new_ids, &entered, point);
      if wants(tree, &left, EventInterest::LEAVE) {
        let target = old_ids.last().copied().unwrap_or(0);
        events.push(delivery(RoutedKind::Leave, key, point, modifiers, left, left_locals, left_parents, target));
      }
      if wants(tree, &entered, EventInterest::ENTER) {
        let target = new_ids.last().copied().unwrap_or(0);
        events.push(delivery(
          RoutedKind::Enter,
          key,
          point,
          modifiers,
          entered,
          entered_locals,
          entered_parents,
          target,
        ));
      }
    }
    self.hovered.insert(key, new_ids);
    events
  }
}
