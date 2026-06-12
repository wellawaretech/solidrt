use crate::frame::{EngineState, InputState};
use crate::overlay;
use crate::plugins;
use crate::rendertree::{self, PlatformContext};
use crate::AlloyContext;
use alloy::impellers::{DisplayList, DisplayListBuilder};
use flux::{
  emit_event,
  rquickjs::{Ctx as QuickJsContext, Function},
};
use std::cell::RefCell;
use std::sync::Arc;

// The last built display list together with the inputs it was built from.
// While all of these are unchanged, a requested frame is present-only (texture
// content changed, e.g. a camera upload): re-rendering the same display list
// samples the new texture contents, so the build can be skipped.
struct DlCache {
  dl: DisplayList,
  revision: u64,
  textures_generation: u64,
  window: (f32, f32),
  scale: f32,
}

pub fn init(
  qtx: QuickJsContext<'_>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  engine_state: Arc<EngineState>,
) {
  let stats = RefCell::new(overlay::Stats::new());
  let cache: RefCell<Option<DlCache>> = RefCell::new(None);
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

    // Present-only reuse: nothing that feeds the display list changed, so
    // resubmit the cached one instead of rebuilding. Layout, postLayout and
    // hover refresh are skipped too - the tree and window are unchanged.
    // Bypassed in record mode to keep its captures identical to a rebuild
    // (the overlay would otherwise freeze mid-recording).
    if !platform.always_render() {
      if let Some(c) = cache.borrow().as_ref() {
        if c.revision == tree.0.borrow().revision()
          && c.textures_generation == atx.textures.generation()
          && c.window == platform.window_size()
          && c.scale == platform.display_scale()
        {
          atx.submit(c.dl.clone()).expect("Failed to submit display list");
          return;
        }
      }
    }

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

    let paint_stats = {
      let mut tree_b = tree.0.borrow_mut();
      rendertree::composite::paint_phase(&mut builder, &mut tree_b, &platform, &*atx)
    };

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
      paint_stats,
    );

    if let Some(dl) = builder.build() {
      // Sample the cache key after building: postLayout handlers may have
      // mutated the tree, and a first build can itself create textures.
      *cache.borrow_mut() = Some(DlCache {
        dl: dl.clone(),
        revision: tree.0.borrow().revision(),
        textures_generation: atx.textures.generation(),
        window: platform.window_size(),
        scale: platform.display_scale(),
      });
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .expect("create draw");

  let globals = qtx.globals();
  globals.set("draw", draw_fn).expect("set draw");
}
