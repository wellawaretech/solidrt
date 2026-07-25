use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use alloy::rendertree::{
  hit::{locals_along_path, path_diff, DefaultHitTester, HitEntry, HitTester},
  RenderTree, XY,
};
use alloy::{Modifiers, PointerType};
use rquickjs::{Array, Ctx, JsLifetime, Object};

use crate::emit_event;

pub type PointerKey = (PointerType, u64);

pub enum InputEvent {
  PointerMove { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, modifiers: Modifiers },
  PointerDown { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  PointerUp { pointer_id: u64, pointer_type: PointerType, button: u8, x: f32, y: f32, modifiers: Modifiers },
  Wheel { pointer_id: u64, pointer_type: PointerType, x: f32, y: f32, delta_x: f32, delta_y: f32, modifiers: Modifiers },
}

// Per-pointer path state aimed at this engine's render tree: the hovered path
// (enter/leave diffing) and the path frozen at pointerDown. While a pointer
// has an active down, its moves and up dispatch along the frozen path no
// matter where the pointer currently is, so every node under the original
// down observes the whole gesture (drags keep working off-element, ancestor
// recognizers see the moves they need). Freezing lives here rather than in JS
// so exact local coordinates can be projected along the frozen chain. Lives
// as ctx userdata so its lifetime is the engine's: a reload builds a fresh
// engine (and tree), and the old paths (whose node ids would dangle) die with
// it.
#[derive(Default)]
struct PointerPaths {
  hovered: HashMap<PointerKey, Vec<u64>>,
  down: HashMap<PointerKey, Vec<u64>>,
}

#[derive(Clone, JsLifetime)]
struct EngineState(#[qjs(skip_trace)] Rc<RefCell<PointerPaths>>);

impl EngineState {
  fn hovered_path(&self, key: PointerKey) -> Vec<u64> {
    self.0.borrow().hovered.get(&key).cloned().unwrap_or_default()
  }

  fn set_hovered_path(&self, key: PointerKey, path: Vec<u64>) {
    self.0.borrow_mut().hovered.insert(key, path);
  }

  fn remove_hovered_path(&self, key: PointerKey) {
    self.0.borrow_mut().hovered.remove(&key);
  }

  fn down_path(&self, key: PointerKey) -> Option<Vec<u64>> {
    self.0.borrow().down.get(&key).cloned()
  }

  fn set_down_path(&self, key: PointerKey, path: Vec<u64>) {
    self.0.borrow_mut().down.insert(key, path);
  }

  fn take_down_path(&self, key: PointerKey) -> Option<Vec<u64>> {
    self.0.borrow_mut().down.remove(&key)
  }
}

/// Store fresh hover-tracking state in userdata, before any dispatch.
pub fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(EngineState(Rc::new(RefCell::new(PointerPaths::default())))).expect("store input state");
}

// `locals` and `parents` are index-parallel with `target_ids`: the pointer
// position in each target's own coordinate frame, and in its path-parent's
// frame (the window frame for the root). JS collapses them to per-handler
// scalars during the dispatch walk. `target` is the deepest node of the full
// path the event was computed from (also for enter/leave subsets).
fn build_pointer_obj<'js>(
  ctx: &Ctx<'js>,
  pointer_id: u64,
  pointer_type: PointerType,
  x: f32,
  y: f32,
  modifiers: Modifiers,
  target_ids: &[u64],
  locals: &[XY],
  parents: &[XY],
  target: u64,
) -> Object<'js> {
  debug_assert_eq!(target_ids.len(), locals.len(), "targets and locals must stay parallel");
  debug_assert_eq!(target_ids.len(), parents.len(), "targets and parents must stay parallel");
  let obj = Object::new(ctx.clone()).expect("pointer obj");
  let targets = Array::new(ctx.clone()).expect("targets array");
  let local_xs = Array::new(ctx.clone()).expect("localX array");
  let local_ys = Array::new(ctx.clone()).expect("localY array");
  let parent_xs = Array::new(ctx.clone()).expect("parentX array");
  let parent_ys = Array::new(ctx.clone()).expect("parentY array");
  for (i, (&id, (local, parent))) in target_ids.iter().zip(locals.iter().zip(parents)).enumerate() {
    targets.set(i, id).expect("set target");
    local_xs.set(i, local.x).expect("set localX");
    local_ys.set(i, local.y).expect("set localY");
    parent_xs.set(i, parent.x).expect("set parentX");
    parent_ys.set(i, parent.y).expect("set parentY");
  }
  obj.set("targets", targets).expect("set targets");
  obj.set("localX", local_xs).expect("set localX");
  obj.set("localY", local_ys).expect("set localY");
  obj.set("parentX", parent_xs).expect("set parentX");
  obj.set("parentY", parent_ys).expect("set parentY");
  obj.set("target", target).expect("set target");
  obj.set("pointerId", pointer_id).expect("set pointerId");
  obj.set("pointerType", pointer_type.as_str()).expect("set pointerType");
  obj.set("clientX", x).expect("set clientX");
  obj.set("clientY", y).expect("set clientY");
  obj.set("shiftKey", modifiers.shift).expect("set shiftKey");
  obj.set("ctrlKey", modifiers.ctrl).expect("set ctrlKey");
  obj.set("altKey", modifiers.alt).expect("set altKey");
  obj.set("metaKey", modifiers.meta).expect("set metaKey");
  obj
}

fn split_path(path: Vec<HitEntry>) -> (Vec<u64>, Vec<XY>) {
  path.into_iter().map(|(id, _, local)| (id, local)).unzip()
}

// Parent-frame points for a full root->leaf chain: the window point for the
// root, each node's local for its child.
fn parent_locals(point: XY, locals: &[XY]) -> Vec<XY> {
  std::iter::once(point).chain(locals.iter().copied()).take(locals.len()).collect()
}

// Targets + exact locals for an event routed along a stored chain: project the
// point along it; ids past a truncated projection (their nodes were removed
// mid-gesture) are dropped so the arrays stay parallel.
fn routed(tree: &RenderTree, chain: Vec<u64>, point: XY) -> (Vec<u64>, Vec<XY>) {
  let locals = locals_along_path(tree, &chain, point);
  let mut ids = chain;
  ids.truncate(locals.len());
  (ids, locals)
}

// Locals and parent-frame points for `subset` of `chain` (a path_diff result
// or a reversed leave list), by projecting along the full chain and picking
// entries out. An id the projection could not reach (its node was removed)
// keeps the window point; nothing can observe it - a removed node has no
// handlers.
fn pick_locals(tree: &RenderTree, chain: &[u64], subset: &[u64], point: XY) -> (Vec<XY>, Vec<XY>) {
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

/// Hit test the current tree (last computed layout) and emit the matching JS
/// pointer event. Runs when the event arrives, not per frame, so input keeps
/// working when no frame is being produced. Handlers that mutate state request
/// the next frame through their ffi calls.
pub fn dispatch(ctx: &Ctx<'_>, event: InputEvent) {
  let tree = ctx.userdata::<super::tree::SharedRenderTree>().expect("render tree userdata");
  let state = ctx.userdata::<EngineState>().expect("input state userdata");
  match event {
    InputEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => {
      let key = (pointer_type, pointer_id);
      let point = XY::new(x, y);
      let (live_ids, live_locals) = split_path(DefaultHitTester.hit_test(&tree.0.borrow(), point));
      let (ids, locals) = match state.down_path(key) {
        Some(frozen) => routed(&tree.0.borrow(), frozen, point),
        None => (live_ids.clone(), live_locals),
      };
      let parents = parent_locals(point, &locals);
      let target = ids.last().copied().unwrap_or(0);
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids, &locals, &parents, target);
      emit_event(ctx, "pointerMove", obj);
      update_hover(ctx, &tree, &state, key, x, y, modifiers, live_ids);
    }
    InputEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
      let point = XY::new(x, y);
      let (ids, locals) = split_path(DefaultHitTester.hit_test(&tree.0.borrow(), point));
      if ids.is_empty() {
        return;
      }
      state.set_down_path((pointer_type, pointer_id), ids.clone());
      let parents = parent_locals(point, &locals);
      let target = ids.last().copied().unwrap_or(0);
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids, &locals, &parents, target);
      obj.set("button", button).expect("set button");
      emit_event(ctx, "pointerDown", obj);
    }
    InputEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
      let key = (pointer_type, pointer_id);
      let point = XY::new(x, y);
      let (ids, locals) = match state.take_down_path(key) {
        Some(frozen) => routed(&tree.0.borrow(), frozen, point),
        None => split_path(DefaultHitTester.hit_test(&tree.0.borrow(), point)),
      };
      let parents = parent_locals(point, &locals);
      let target = ids.last().copied().unwrap_or(0);
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids, &locals, &parents, target);
      obj.set("button", button).expect("set button");
      emit_event(ctx, "pointerUp", obj);

      // For touch, the pointer ends here. Emit a final pointerLeave for
      // anything still in its hovered path so JS can clean up, then drop the
      // hover entry to prevent it from leaking across future touches.
      if pointer_type == PointerType::Touch {
        let old_ids = state.hovered_path(key);
        if !old_ids.is_empty() {
          let leave: Vec<u64> = old_ids.iter().rev().copied().collect();
          let (leave_locals, leave_parents) = pick_locals(&tree.0.borrow(), &old_ids, &leave, point);
          let target = old_ids.last().copied().unwrap_or(0);
          let obj =
            build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &leave, &leave_locals, &leave_parents, target);
          emit_event(ctx, "pointerLeave", obj);
        }
        state.remove_hovered_path(key);
      }
    }
    InputEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
      let point = XY::new(x, y);
      let (ids, locals) = split_path(DefaultHitTester.hit_test(&tree.0.borrow(), point));
      let parents = parent_locals(point, &locals);
      let target = ids.last().copied().unwrap_or(0);
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids, &locals, &parents, target);
      obj.set("deltaX", delta_x).expect("set deltaX");
      obj.set("deltaY", delta_y).expect("set deltaY");
      emit_event(ctx, "wheel", obj);
    }
  }
}

/// Re-run the hover diff for every live pointer. Called after each produced
/// frame: layout changes can move elements under a stationary cursor, which
/// arrival-time dispatch cannot see. `pointers` is the runner's device-position
/// snapshot; that bookkeeping outlives any single engine, so it stays on the
/// runner's side of the boundary.
pub fn refresh_hover(ctx: &Ctx<'_>, pointers: Vec<(PointerKey, (f32, f32))>, modifiers: Modifiers) {
  let tree = ctx.userdata::<super::tree::SharedRenderTree>().expect("render tree userdata");
  let state = ctx.userdata::<EngineState>().expect("input state userdata");
  for ((pointer_type, pointer_id), (px, py)) in pointers {
    let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(px, py));
    let new_ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
    update_hover(ctx, &tree, &state, (pointer_type, pointer_id), px, py, modifiers, new_ids);
  }
}

// Emits pointerLeave (deepest-first) and pointerEnter (outermost-in) for the
// hovered-path delta, then stores the new path.
fn update_hover(
  ctx: &Ctx<'_>,
  tree: &super::tree::SharedRenderTree,
  state: &EngineState,
  key: PointerKey,
  x: f32,
  y: f32,
  modifiers: Modifiers,
  new_ids: Vec<u64>,
) {
  let (pointer_type, pointer_id) = key;
  let old_ids = state.hovered_path(key);
  if new_ids != old_ids {
    let (left, entered) = path_diff(&old_ids, &new_ids);
    let point = XY::new(x, y);
    // Both projections resolve before either event fires, against the same
    // tree state the diff was computed from.
    let ((left_locals, left_parents), (entered_locals, entered_parents)) = {
      let t = tree.0.borrow();
      (pick_locals(&t, &old_ids, &left, point), pick_locals(&t, &new_ids, &entered, point))
    };
    if !left.is_empty() {
      let target = old_ids.last().copied().unwrap_or(0);
      let obj =
        build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &left, &left_locals, &left_parents, target);
      emit_event(ctx, "pointerLeave", obj);
    }
    if !entered.is_empty() {
      let target = new_ids.last().copied().unwrap_or(0);
      let obj = build_pointer_obj(
        ctx,
        pointer_id,
        pointer_type,
        x,
        y,
        modifiers,
        &entered,
        &entered_locals,
        &entered_parents,
        target,
      );
      emit_event(ctx, "pointerEnter", obj);
    }
  }
  state.set_hovered_path(key, new_ids);
}
