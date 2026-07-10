use crate::frame::InputState;
use crate::overlay;
use alloy::impellers::{DisplayList, DisplayListBuilder};
use alloy::rendertree::{self, PlatformContext};
use flux::gui::AlloyContext;
use flux::{
  emit_event,
  rquickjs::{
    Ctx as QuickJsContext, Function, JsLifetime,
    module::{Declarations, Exports, ModuleDef},
  },
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
  stats_enabled: bool,
}

// The host state the `srt:render` module binds, stashed in userdata by
// `store_state` before any import so the module's `evaluate` can build
// `renderFrame`. The shared render tree it draws is read separately from
// userdata (stored by flux's tree plugin).
#[derive(Clone, JsLifetime)]
struct RenderState(#[qjs(skip_trace)] Rc<RenderInner>);

struct RenderInner {
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  // Latest stats figures, published every frame for readers outside the draw
  // loop (the dev server's stats query answers from here).
  stats_snapshot: Arc<Mutex<overlay::StatsSnapshot>>,
}

/// Stash the draw bridge's host state in userdata, before any import. The
/// `srt:render` surface is registered separately via `module_override`.
pub fn store_state(
  ctx: &QuickJsContext<'_>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  stats_snapshot: Arc<Mutex<overlay::StatsSnapshot>>,
) {
  ctx
    .store_userdata(RenderState(Rc::new(RenderInner { platform, atx, input_state, stats_snapshot })))
    .expect("store render state");
}

/// The `srt:render` module: `renderFrame()`, the runner's per-frame draw. Not
/// part of `flux:rendertree` because it bundles lattice-only policy (demand
/// gating, display-list reuse, the stats overlay, hover refresh) over the
/// engine-free draw phases in alloy.
pub struct SrtRenderModule;

impl ModuleDef for SrtRenderModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("renderFrame")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &QuickJsContext<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    let state = ctx.userdata::<RenderState>().expect("render state userdata");
    let platform = state.0.platform.clone();
    let atx = state.0.atx.clone();
    let input_state = state.0.input_state.clone();
    let stats_snapshot = state.0.stats_snapshot.clone();

    let stats = RefCell::new(overlay::Stats::new());
    let cache: RefCell<Option<DlCache>> = RefCell::new(None);
    let render_frame_fn = Function::new(ctx.clone(), move |qtx: QuickJsContext<'_>| {
      // JS render-handler cost (onFrame + flush), measured natively: time since
      // the instant stamped before the "render" event, now that the handler has
      // reached draw(). Plus the FFI prop writes that flush produced. Recorded
      // for every frame (gated ones too, since flush still ran).
      let js_ms =
        crate::frame::RENDER_START.with(|c| c.get()).map(|t| t.elapsed().as_secs_f32() * 1000.0).unwrap_or(0.0);
      let set_count = flux::gui::tree::SETPROP_COUNT.with(|c| c.replace(0));
      stats.borrow_mut().record_js(js_ms, set_count);
      // Publish for out-of-loop readers on every frame event, gated or not, so
      // a query sees current numbers even while the demand gate skips draws.
      *stats_snapshot.lock().expect("stats snapshot lock poisoned") =
        stats.borrow().snapshot(platform.fps(), atx.textures.len());

      // Demand-driven gate: when nothing requested a frame, skip it entirely
      // (layout, paint, submit, hover refresh - elements only move when a frame
      // is produced, so hover cannot have changed either). Playback mode renders
      // unconditionally: its capture loop blocks waiting for every frame's
      // display list.
      //
      // The stats overlay is its own demand source: its per-second figures keep
      // changing while the app is idle, so a due overlay forces a frame (and, in
      // the reuse path below, a rebuild) so the HUD stays live. Only matters when
      // the overlay is enabled.
      let stats_on = platform.stats_enabled();
      let overlay_due = stats_on && stats.borrow().overlay_due();
      let requested = platform.take_frame_requested();
      if !requested && !overlay_due && !platform.always_render() {
        stats.borrow_mut().note_skipped();
        return;
      }

      let tree = qtx.userdata::<flux::gui::tree::SharedRenderTree>().expect("render tree userdata");

      // Present-only reuse: nothing that feeds the display list changed, so
      // resubmit the cached one instead of rebuilding. Layout, postLayout and
      // hover refresh are skipped too - the tree and window are unchanged.
      // Bypassed in playback mode to keep its captures identical to a rebuild
      // (the overlay would otherwise freeze mid-recording).
      if !platform.always_render() && !overlay_due {
        if let Some(c) = cache.borrow().as_ref() {
          if c.revision == tree.0.borrow().revision()
            && c.textures_generation == atx.textures.generation()
            && c.window == platform.window_size()
            && c.scale == platform.display_scale()
            && c.stats_enabled == stats_on
          {
            stats.borrow_mut().note_reused();
            atx.submit(c.dl.clone()).expect("Failed to submit display list");
            return;
          }
        }
      }

      let mut builder = DisplayListBuilder::new(None);
      let scale = platform.display_scale();
      builder.scale(scale, scale);

      let mut phases = overlay::FramePhases::default();

      // Layout phase: scope the mut borrow so onLayout handlers (which may
      // call setProperty etc.) don't trip the RefCell.
      let t = Instant::now();
      {
        let mut tree_b = tree.0.borrow_mut();
        rendertree::composite::layout_phase(&mut tree_b, &platform, &*atx);
      }
      phases.layout = t.elapsed();

      // Post-layout hook. Handlers run synchronously and may invalidate the
      // layout cache via setProperty; paint_phase re-runs layout to absorb
      // those changes.
      let t = Instant::now();
      emit_event(&qtx, "postLayout", ());
      phases.post = t.elapsed();

      let t = Instant::now();
      let paint_stats = {
        let mut tree_b = tree.0.borrow_mut();
        rendertree::composite::paint_phase(&mut builder, &mut tree_b, &platform, &*atx)
      };
      phases.paint = t.elapsed();

      // Input dispatch happens on event arrival (flux::gui::input::dispatch);
      // here we only re-check hover, since this frame's layout may have moved
      // elements under a stationary pointer.
      let t = Instant::now();
      flux::gui::input::refresh_hover(&qtx, input_state.pointers(), input_state.modifiers());
      phases.hover = t.elapsed();

      stats.borrow_mut().record_frame(phases);

      if stats_on {
        stats.borrow_mut().draw(
          &mut builder,
          &platform.typography,
          platform.safe_area(),
          platform.fps(),
          paint_stats,
          atx.textures.len(),
        );
      }

      if let Some(dl) = builder.build() {
        // Sample the cache key after building: postLayout handlers may have
        // mutated the tree, and a first build can itself create textures.
        *cache.borrow_mut() = Some(DlCache {
          dl: dl.clone(),
          revision: tree.0.borrow().revision(),
          textures_generation: atx.textures.generation(),
          window: platform.window_size(),
          scale: platform.display_scale(),
          stats_enabled: stats_on,
        });
        atx.submit(dl).expect("Failed to submit display list");
      }
    })?;

    exports.export("renderFrame", render_frame_fn)?;
    Ok(())
  }
}
