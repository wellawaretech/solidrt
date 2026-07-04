use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use alloy::rendertree::{
  hit::{path_diff, DefaultHitTester, HitTester},
  XY,
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

// Hovered node paths per pointer, aimed at this engine's render tree. Lives as
// ctx userdata so its lifetime is the engine's: a reload builds a fresh engine
// (and tree), and the old paths (whose node ids would dangle) die with it.
#[derive(Clone, JsLifetime)]
struct EngineState(#[qjs(skip_trace)] Rc<RefCell<HashMap<PointerKey, Vec<u64>>>>);

impl EngineState {
  fn hovered_path(&self, key: PointerKey) -> Vec<u64> {
    self.0.borrow().get(&key).cloned().unwrap_or_default()
  }

  fn set_hovered_path(&self, key: PointerKey, path: Vec<u64>) {
    self.0.borrow_mut().insert(key, path);
  }

  fn remove_hovered_path(&self, key: PointerKey) {
    self.0.borrow_mut().remove(&key);
  }
}

/// Store fresh hover-tracking state in userdata, before any dispatch.
pub fn store_state(ctx: &Ctx<'_>) {
  ctx.store_userdata(EngineState(Rc::new(RefCell::new(HashMap::new())))).expect("store input state");
}

fn build_pointer_obj<'js>(
  ctx: &Ctx<'js>,
  pointer_id: u64,
  pointer_type: PointerType,
  x: f32,
  y: f32,
  modifiers: Modifiers,
  target_ids: &[u64],
) -> Object<'js> {
  let obj = Object::new(ctx.clone()).expect("pointer obj");
  let targets = Array::new(ctx.clone()).expect("targets array");
  for (i, &id) in target_ids.iter().enumerate() {
    targets.set(i, id).expect("set target");
  }
  obj.set("targets", targets).expect("set targets");
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

/// Hit test the current tree (last computed layout) and emit the matching JS
/// pointer event. Runs when the event arrives, not per frame, so input keeps
/// working when no frame is being produced. Handlers that mutate state request
/// the next frame through their ffi calls.
pub fn dispatch(ctx: &Ctx<'_>, event: InputEvent) {
  let tree = ctx.userdata::<super::tree::SharedRenderTree>().expect("render tree userdata");
  let state = ctx.userdata::<EngineState>().expect("input state userdata");
  match event {
    InputEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => {
      let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
      let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids);
      emit_event(ctx, "pointerMove", obj);
      update_hover(ctx, &state, (pointer_type, pointer_id), x, y, modifiers, ids);
    }
    InputEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
      let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
      if path.is_empty() {
        return;
      }
      let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids);
      obj.set("button", button).expect("set button");
      emit_event(ctx, "pointerDown", obj);
    }
    InputEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
      let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
      let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids);
      obj.set("button", button).expect("set button");
      emit_event(ctx, "pointerUp", obj);

      // For touch, the pointer ends here. Emit a final pointerLeave for
      // anything still in its hovered path so JS can clean up, then drop the
      // hover entry to prevent it from leaking across future touches.
      if pointer_type == PointerType::Touch {
        let key = (pointer_type, pointer_id);
        let old_ids = state.hovered_path(key);
        if !old_ids.is_empty() {
          let leave: Vec<u64> = old_ids.iter().rev().copied().collect();
          let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &leave);
          emit_event(ctx, "pointerLeave", obj);
        }
        state.remove_hovered_path(key);
      }
    }
    InputEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
      let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
      let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &ids);
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
    update_hover(ctx, &state, (pointer_type, pointer_id), px, py, modifiers, new_ids);
  }
}

// Emits pointerLeave (deepest-first) and pointerEnter (outermost-in) for the
// hovered-path delta, then stores the new path.
fn update_hover(
  ctx: &Ctx<'_>,
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
    if !left.is_empty() {
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &left);
      emit_event(ctx, "pointerLeave", obj);
    }
    if !entered.is_empty() {
      let obj = build_pointer_obj(ctx, pointer_id, pointer_type, x, y, modifiers, &entered);
      emit_event(ctx, "pointerEnter", obj);
    }
  }
  state.set_hovered_path(key, new_ids);
}
