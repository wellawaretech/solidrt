use crate::frame::InputState;
use crate::overlay;
use alloy::impellers::{DisplayList, DisplayListBuilder};
use alloy::rendertree::{self, PlatformContext};
use flux::gui::AlloyContext;
use flux::{
  emit_event,
  rquickjs::{
    module::{Declarations, Exports, ModuleDef},
    Ctx as QuickJsContext, Function, JsLifetime,
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
// userdata (stored by flux's tree plugin). Also holds the draw loop's own
// frame-to-frame state (stats, display-list cache) so the body is callable
// both from the JS export and natively (see `render_now`).
#[derive(Clone, JsLifetime)]
struct RenderState(#[qjs(skip_trace)] Rc<RenderInner>);

struct RenderInner {
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  // Latest stats figures, published every frame for readers outside the draw
  // loop (the dev server's stats query answers from here).
  stats_snapshot: Arc<Mutex<overlay::StatsSnapshot>>,
  stats: RefCell<overlay::Stats>,
  cache: RefCell<Option<DlCache>>,
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
    .store_userdata(RenderState(Rc::new(RenderInner {
      platform,
      atx,
      input_state,
      stats_snapshot,
      stats: RefCell::new(overlay::Stats::new()),
      cache: RefCell::new(None),
    })))
    .expect("store render state");
}

/// Run the per-frame draw directly - the same body the JS `renderFrame`
/// export wraps. The paused frame path (dev-tool clock control) calls this so
/// snapshot captures and damage-driven presents keep working while the render
/// event is gated; the demand gate inside makes an idle call free. No-op
/// before `store_state` ran.
pub fn render_now(ctx: &QuickJsContext<'_>) {
  if let Some(state) = ctx.userdata::<RenderState>() {
    state.0.render(ctx);
  }
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
    let state = ctx.userdata::<RenderState>().expect("render state userdata").clone();
    let render_frame_fn = Function::new(ctx.clone(), move |qtx: QuickJsContext<'_>| state.0.render(&qtx))?;

    exports.export("renderFrame", render_frame_fn)?;
    Ok(())
  }
}

impl RenderInner {
  fn render(&self, qtx: &QuickJsContext<'_>) {
    let platform = &*self.platform;
    let atx = &*self.atx;
    let input_state = &*self.input_state;
    let stats_snapshot = &self.stats_snapshot;
    let stats = &self.stats;
    let cache = &self.cache;
    {
      // JS render-handler cost (onFrame + flush), measured natively: time since
      // the instant stamped before the "render" event, now that the handler has
      // reached draw(). Plus the FFI prop writes that flush produced. Recorded
      // for every frame (gated ones too, since flush still ran). Consumed, so a
      // native call with no render event (the paused path) reads zero instead
      // of the stale stamp of the last delivered frame.
      let js_ms =
        crate::frame::RENDER_START.with(|c| c.replace(None)).map(|t| t.elapsed().as_secs_f32() * 1000.0).unwrap_or(0.0);
      let set_count = flux::gui::tree::SETPROP_COUNT.with(|c| c.replace(0));
      stats.borrow_mut().record_js(js_ms, set_count);
      // Publish for out-of-loop readers on every frame event, gated or not, so
      // a query sees current numbers even while the demand gate skips draws.
      *stats_snapshot.lock().expect("stats snapshot lock poisoned") =
        stats.borrow().snapshot(platform.fps(), atx.textures.len());
    }

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
    // (the overlay would otherwise freeze mid-recording), and when captures
    // are pending: they are serviced by the paint walk, which the reuse
    // path skips, so reusing would strand them.
    if !platform.always_render() && !overlay_due && !atx.has_pending_captures() {
      if let Some(c) = cache.borrow().as_ref() {
        if c.revision == tree.0.borrow().revision()
          && c.textures_generation == atx.textures.generation()
          && c.window == platform.window_size()
          && c.scale == platform.display_scale()
          && c.stats_enabled == stats_on
        {
          // A window-shader prop write lands here (Damage::Present bumps no
          // revision): flush it ahead of the frame, the ordering the build
          // path gets from the paint walk.
          if let Some(change) = tree.0.borrow_mut().take_pending_window_shader() {
            if let Err(e) = atx.set_window_shader(change) {
              log::warn!("[render] window shader: {e}");
            }
          }
          stats.borrow_mut().note_reused();
          // Clean resubmit: the raster side may run only the shader pass
          // over its retained layer (see Context::submit_clean).
          atx.submit_clean(c.dl.clone()).expect("Failed to submit display list");
          // The reuse path skips paint_phase, whose end-of-frame sweep
          // reclaims deferred destroys - run it here too so a destroy with
          // no other tree change (its requested frame lands in this path)
          // is not stranded until the next rebuild. The cached list's Rc'd
          // Impeller handles keep its textures alive regardless.
          if atx.has_pending_destroys() {
            atx.reclaim_destroyed(&tree.0.borrow().referenced_texture_ids());
          }
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
      rendertree::composite::layout_phase(&mut tree_b, platform, atx);
    }
    phases.layout = t.elapsed();

    // Post-layout hook. Handlers run synchronously and may invalidate the
    // layout cache via setProperty; paint_phase re-runs layout to absorb
    // those changes.
    let t = Instant::now();
    emit_event(qtx, "postLayout", ());
    phases.post = t.elapsed();

    let t = Instant::now();
    let paint_stats = {
      let mut tree_b = tree.0.borrow_mut();
      rendertree::composite::paint_phase(&mut builder, &mut tree_b, platform, atx)
    };
    phases.paint = t.elapsed();

    // Input dispatch happens on event arrival (flux::gui::input::dispatch);
    // here we only re-check hover, since this frame's layout may have moved
    // elements under a stationary pointer.
    let t = Instant::now();
    flux::gui::input::refresh_hover(qtx, input_state.pointers(), input_state.modifiers());
    phases.hover = t.elapsed();

    {
      let mut s = stats.borrow_mut();
      s.record_frame(phases);
      // Taken after paint so the counters cover the whole rebuild (paint
      // shapes paragraphs too), plus the writes that led into it.
      s.record_layout_activity(tree.0.borrow().node_count(), rendertree::counters::take());
    }

    if stats_on {
      stats.borrow_mut().draw(
        &mut builder,
        &platform.typography(),
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
  }
}
