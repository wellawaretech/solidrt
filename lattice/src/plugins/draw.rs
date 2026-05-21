use crate::frame::{EngineState, InputEvent, InputState};
use crate::overlay;
use crate::rendertree::{self, hit::{DefaultHitTester, HitEntry, HitTester}, PlatformContext, XY};
use crate::AlloyContext;
use crate::plugins;
use alloy::impellers::DisplayListBuilder;
use alloy::{Modifiers, PointerType};
use flux::{emit_event, rquickjs::{Array, Ctx as QuickJsContext, Function, Object}};
use std::sync::Arc;

fn build_pointer_obj<'js>(
  qtx: &QuickJsContext<'js>,
  pointer_id: u64,
  pointer_type: PointerType,
  x: f32,
  y: f32,
  modifiers: Modifiers,
  target_ids: &[u64],
) -> Object<'js> {
  let obj = Object::new(qtx.clone()).expect("pointer obj");
  let targets = Array::new(qtx.clone()).expect("targets array");
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

pub fn init(
  qtx: QuickJsContext<'_>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  engine_state: Arc<EngineState>,
) {
  let draw_fn = Function::new(qtx.clone(), move |qtx: QuickJsContext<'_>| {
    let tree = qtx.userdata::<plugins::tree::SharedRenderTree>().unwrap();
    let mut builder = DisplayListBuilder::new(None);
    let scale = platform.display_scale();
    builder.scale(scale, scale);
    rendertree::composite::composite(&mut builder, &mut tree.0.borrow_mut(), &platform);

    for event in engine_state.drain_input() {
      match event {
        InputEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => {
          let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
          let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
          let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, x, y, modifiers, &ids);
          emit_event(&qtx, "pointerMove", obj);
        }
        InputEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
          let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
          if path.is_empty() { continue; }
          let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
          let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, x, y, modifiers, &ids);
          obj.set("button", button).expect("set button");
          emit_event(&qtx, "pointerDown", obj);
        }
        InputEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
          let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
          let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
          let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, x, y, modifiers, &ids);
          obj.set("button", button).expect("set button");
          emit_event(&qtx, "pointerUp", obj);

          // For touch, the pointer ends here. Emit a final pointerLeave
          // for anything still in its hovered path so JS can clean up,
          // then drop the hover entry to prevent it from leaking across
          // future touches.
          if pointer_type == PointerType::Touch {
            let key = (pointer_type, pointer_id);
            let old_ids = engine_state.hovered_path(key);
            if !old_ids.is_empty() {
              let leave: Vec<u64> = old_ids.iter().rev().copied().collect();
              let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, x, y, modifiers, &leave);
              emit_event(&qtx, "pointerLeave", obj);
            }
            engine_state.remove_hovered_path(key);
          }
        }
        InputEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
          let path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
          let ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
          let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, x, y, modifiers, &ids);
          obj.set("deltaX", delta_x).expect("set deltaX");
          obj.set("deltaY", delta_y).expect("set deltaY");
          emit_event(&qtx, "wheel", obj);
        }
      }
    }

    let modifiers = input_state.modifiers();
    for ((pointer_type, pointer_id), (px, py)) in input_state.pointers() {
      let path: Vec<HitEntry> = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(px, py));
      let new_ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
      let key = (pointer_type, pointer_id);
      let old_ids = engine_state.hovered_path(key);

      if new_ids != old_ids {
        let mut diverge = 0;
        while diverge < old_ids.len() && diverge < new_ids.len() && old_ids[diverge] == new_ids[diverge] {
          diverge += 1;
        }

        let left: Vec<u64> = old_ids[diverge..].iter().rev().copied().collect();
        if !left.is_empty() {
          let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, px, py, modifiers, &left);
          emit_event(&qtx, "pointerLeave", obj);
        }

        let entered: Vec<u64> = new_ids[diverge..].to_vec();
        if !entered.is_empty() {
          let obj = build_pointer_obj(&qtx, pointer_id, pointer_type, px, py, modifiers, &entered);
          emit_event(&qtx, "pointerEnter", obj);
        }
      }

      engine_state.set_hovered_path(key, new_ids);
    }

    // overlay::fps(&mut builder, &platform.typography, platform.safe_area(), platform.fps());

    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .unwrap();

  let globals = qtx.globals();
  globals.set("draw", draw_fn).unwrap();
}