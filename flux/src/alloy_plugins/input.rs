use std::cell::RefCell;
use std::rc::Rc;

use alloy::rendertree::{PointerRouter, RoutedKind, RoutedPointer};
use alloy::Modifiers;
use rquickjs::{Array, Ctx, JsLifetime, Object};

use crate::emit_event;

pub use alloy::rendertree::{InputEvent, PointerKey};

// The router holds per-pointer paths whose node ids belong to this engine's
// tree. It lives as ctx userdata so its lifetime is the engine's: a reload
// builds a fresh engine (and tree), and the old paths (whose node ids would
// dangle) die with it.
#[derive(Clone, JsLifetime)]
struct EngineState(#[qjs(skip_trace)] Rc<RefCell<PointerRouter>>);

/// Store a fresh pointer router in userdata, before any dispatch.
pub fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(EngineState(Rc::new(RefCell::new(PointerRouter::default())))).expect("store input state");
}

// One routed delivery as the JS pointer event object. `targets`, `localX/Y`
// and `parentX/Y` are index-parallel (the router guarantees it); JS collapses
// them to per-handler scalars during the dispatch walk.
fn build_pointer_obj<'js>(ctx: &Ctx<'js>, ev: &RoutedPointer) -> Object<'js> {
  let obj = Object::new(ctx.clone()).expect("pointer obj");
  let targets = Array::new(ctx.clone()).expect("targets array");
  let local_xs = Array::new(ctx.clone()).expect("localX array");
  let local_ys = Array::new(ctx.clone()).expect("localY array");
  let parent_xs = Array::new(ctx.clone()).expect("parentX array");
  let parent_ys = Array::new(ctx.clone()).expect("parentY array");
  for (i, (&id, (local, parent))) in ev.targets.iter().zip(ev.locals.iter().zip(&ev.parents)).enumerate() {
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
  obj.set("target", ev.target).expect("set target");
  obj.set("pointerId", ev.pointer_id).expect("set pointerId");
  obj.set("pointerType", ev.pointer_type.as_str()).expect("set pointerType");
  obj.set("clientX", ev.position.x).expect("set clientX");
  obj.set("clientY", ev.position.y).expect("set clientY");
  obj.set("shiftKey", ev.modifiers.shift).expect("set shiftKey");
  obj.set("ctrlKey", ev.modifiers.ctrl).expect("set ctrlKey");
  obj.set("altKey", ev.modifiers.alt).expect("set altKey");
  obj.set("metaKey", ev.modifiers.meta).expect("set metaKey");
  // movementX/movementY are always present (stable shape); non-move kinds
  // report 0, matching browser practice.
  let (dx, dy) = match ev.kind {
    RoutedKind::Move { dx, dy } => (dx, dy),
    _ => (0.0, 0.0),
  };
  obj.set("movementX", dx).expect("set movementX");
  obj.set("movementY", dy).expect("set movementY");
  match ev.kind {
    RoutedKind::Down { button } | RoutedKind::Up { button } => obj.set("button", button).expect("set button"),
    RoutedKind::Wheel { delta_x, delta_y } => {
      obj.set("deltaX", delta_x).expect("set deltaX");
      obj.set("deltaY", delta_y).expect("set deltaY");
    }
    _ => {}
  }
  obj
}

fn emit_routed(ctx: &Ctx<'_>, events: Vec<RoutedPointer>) {
  for ev in events {
    let name = match ev.kind {
      RoutedKind::Move { .. } => "pointerMove",
      RoutedKind::Down { .. } => "pointerDown",
      RoutedKind::Up { .. } => "pointerUp",
      RoutedKind::Enter => "pointerEnter",
      RoutedKind::Leave => "pointerLeave",
      RoutedKind::Wheel { .. } => "wheel",
    };
    let obj = build_pointer_obj(ctx, &ev);
    emit_event(ctx, name, obj);
  }
}

/// Route one pointer event against the current tree (last computed layout)
/// and emit the matching JS pointer events. Runs when the event arrives, not
/// per frame, so input keeps working when no frame is being produced.
/// Handlers that mutate state request the next frame through their ffi calls.
pub fn dispatch(ctx: &Ctx<'_>, event: InputEvent) {
  let tree = ctx.userdata::<super::tree::SharedRenderTree>().expect("render tree userdata");
  let state = ctx.userdata::<EngineState>().expect("input state userdata");
  // Routing resolves fully (tree and router borrows released) before any
  // handler runs: handlers mutate the tree through their own ffi calls.
  let events = state.0.borrow_mut().dispatch(&tree.0.borrow(), event);
  emit_routed(ctx, events);
}

/// The frame's move-batch terminator: emitted after all of a frame's
/// resampled moves have dispatched, before rAF and the render event in the
/// same job. Every pointer position JS holds is the same age at this point,
/// so multi-pointer recognizers subscribe to it as their "measure once per
/// frame" signal instead of measuring per move (which would pair one fresh
/// position with stale ones). Fires only on frames that had moves.
pub fn frame_end(ctx: &Ctx<'_>) {
  emit_event(ctx, "pointerFrame", ());
}

/// Re-run the hover diff for every live pointer. Called after each produced
/// frame: layout changes can move elements under a stationary cursor, which
/// arrival-time dispatch cannot see. `pointers` is the runner's
/// device-position snapshot; that bookkeeping outlives any single engine, so
/// it stays on the runner's side of the boundary.
pub fn refresh_hover(ctx: &Ctx<'_>, pointers: Vec<(PointerKey, (f32, f32))>, modifiers: Modifiers) {
  let tree = ctx.userdata::<super::tree::SharedRenderTree>().expect("render tree userdata");
  let state = ctx.userdata::<EngineState>().expect("input state userdata");
  let events = state.0.borrow_mut().refresh_hover(&tree.0.borrow(), pointers, modifiers);
  emit_routed(ctx, events);
}