use crate::frame::{FrameState, InputEvent};
use crate::rendertree::{self, hit::{DefaultHitTester, HitEntry, HitTester}, PlatformContext, XY};
use crate::AlloyContext;
use crate::plugins;
use alloy::impellers::DisplayListBuilder;
use flux::{emit_event, rquickjs::{Array, Ctx as QuickJsContext, Function, Object}};
use std::sync::Arc;

pub fn init(qtx: QuickJsContext<'_>, platform: Arc<PlatformContext>, atx: AlloyContext, frame_state: Arc<FrameState>) {
  let draw_fn = Function::new(qtx.clone(), move |qtx: QuickJsContext<'_>| {
    let tree = qtx.userdata::<plugins::tree::SharedRenderTree>().unwrap();
    let mut builder = DisplayListBuilder::new(None);
    rendertree::composite::composite(&mut builder, &mut tree.0.borrow_mut(), &platform);

    for event in frame_state.drain_input() {
      match event {
        InputEvent::PointerMove { x, y } => {
          frame_state.set_pointer_pos(x, y);
          let move_path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
          let obj = Object::new(qtx.clone()).expect("pointerMove obj");
          let targets = Array::new(qtx.clone()).expect("pointerMove targets");
          for (i, &(id, _, _)) in move_path.iter().enumerate() { targets.set(i, id).expect("set"); }
          obj.set("targets", targets).expect("set targets");
          obj.set("clientX", x).expect("set clientX");
          obj.set("clientY", y).expect("set clientY");
          emit_event(&qtx, "pointerMove", obj);
        }
        InputEvent::PointerDown { button, x, y } => {
          let down_path = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(x, y));
          if down_path.is_empty() { continue; }
          let obj = Object::new(qtx.clone()).expect("pointerDown obj");
          let targets = Array::new(qtx.clone()).expect("pointerDown targets");
          for (i, &(id, _, _)) in down_path.iter().enumerate() { targets.set(i, id).expect("set"); }
          obj.set("targets", targets).expect("set targets");
          obj.set("clientX", x).expect("set clientX");
          obj.set("clientY", y).expect("set clientY");
          obj.set("button", button).expect("set button");
          emit_event(&qtx, "pointerDown", obj);
        }
      }
    }

    let (px, py) = frame_state.pointer_pos();
    let path: Vec<HitEntry> = DefaultHitTester.hit_test(&tree.0.borrow(), XY::new(px, py));

    let new_ids: Vec<u64> = path.iter().map(|&(id, _, _)| id).collect();
    let old_ids = frame_state.hovered_path();

    if new_ids != old_ids {
      let mut diverge = 0;
      while diverge < old_ids.len() && diverge < new_ids.len() && old_ids[diverge] == new_ids[diverge] {
        diverge += 1;
      }

      let left: Vec<u64> = old_ids[diverge..].iter().rev().copied().collect();
      if !left.is_empty() {
        let obj = Object::new(qtx.clone()).expect("pointerLeave obj");
        let targets = Array::new(qtx.clone()).expect("pointerLeave targets");
        for (i, &id) in left.iter().enumerate() { targets.set(i, id).expect("set"); }
        obj.set("targets", targets).expect("set targets");
        obj.set("clientX", px).expect("set clientX");
        obj.set("clientY", py).expect("set clientY");
        emit_event(&qtx, "pointerLeave", obj);
      }

      let entered = &new_ids[diverge..];
      if !entered.is_empty() {
        let obj = Object::new(qtx.clone()).expect("pointerEnter obj");
        let targets = Array::new(qtx.clone()).expect("pointerEnter targets");
        for (i, &id) in entered.iter().enumerate() { targets.set(i, id).expect("set"); }
        obj.set("targets", targets).expect("set targets");
        obj.set("clientX", px).expect("set clientX");
        obj.set("clientY", py).expect("set clientY");
        emit_event(&qtx, "pointerEnter", obj);
      }
    }

    frame_state.set_hovered_path(new_ids);

    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .unwrap();

  let globals = qtx.globals();
  globals.set("draw", draw_fn).unwrap();
}
