use crate::frame::{EngineState, InputState};
use crate::overlay;
use crate::plugins;
use crate::rendertree::{self, PlatformContext};
use crate::AlloyContext;
use alloy::impellers::DisplayListBuilder;
use flux::{
  emit_event,
  rquickjs::{Ctx as QuickJsContext, Function},
};
use std::sync::Arc;

pub fn init(
  qtx: QuickJsContext<'_>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  engine_state: Arc<EngineState>,
) {
  let stats = std::cell::RefCell::new(overlay::Stats::new());
  let draw_fn = Function::new(qtx.clone(), move |qtx: QuickJsContext<'_>| {
    // Demand-driven gate: when nothing requested a frame, skip it entirely
    // (layout, paint, submit, hover refresh - elements only move when a frame
    // is produced, so hover cannot have changed either). Record mode renders
    // unconditionally: its capture loop blocks waiting for every frame's
    // display list.
    let requested = platform.take_frame_requested();
    if !requested && !platform.always_render() {
      return;
    }

    let tree = qtx.userdata::<plugins::tree::SharedRenderTree>().expect("render tree userdata");
    let mut builder = DisplayListBuilder::new(None);
    let scale = platform.display_scale();
    builder.scale(scale, scale);

    // Layout phase: scope the mut borrow so onLayout handlers (which may
    // call setProperty etc.) don't trip the RefCell.
    {
      let mut tree_b = tree.0.borrow_mut();
      rendertree::composite::layout_phase(&mut tree_b, &platform, &*atx);
    }

    // Post-layout hook. Handlers run synchronously and may invalidate the
    // layout cache via setProperty; paint_phase re-runs layout to absorb
    // those changes.
    emit_event(&qtx, "postLayout", ());

    {
      let mut tree_b = tree.0.borrow_mut();
      rendertree::composite::paint_phase(&mut builder, &mut tree_b, &platform, &*atx);
    }

    // Input dispatch happens on event arrival (plugins::input::dispatch);
    // here we only re-check hover, since this frame's layout may have moved
    // elements under a stationary pointer.
    plugins::input::refresh_hover(&qtx, &input_state, &engine_state);

    stats.borrow_mut().draw(
      &mut builder,
      &platform.typography,
      platform.safe_area(),
      platform.fps(),
      platform.requests_per_second(),
    );

    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .expect("create draw");

  let globals = qtx.globals();
  globals.set("draw", draw_fn).expect("set draw");
}
