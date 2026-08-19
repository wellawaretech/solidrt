use crate::frame_history::{FrameHistory, FrameRecord};
use crate::overlay;
use crate::stats;
use alloy::InputState;
use alloy::rendertree::{self, Commit, FrameDriver, PlatformContext};
use flux::gui::AlloyContext;
use flux::{
  emit_event, CtxLogger,
  rquickjs::{
    module::{Declarations, Exports, ModuleDef},
    Ctx as QuickJsContext, Function, JsLifetime,
  },
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Slow-frame warnings are throttled to one per this interval.
const SLOW_WARN_INTERVAL: Duration = Duration::from_secs(1);

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
  stats_snapshot: Arc<Mutex<stats::StatsSnapshot>>,
  stats: RefCell<stats::Stats>,
  // Raw per-rebuild records for the stats query's window summary (worst
  // frame, percentiles), the figures the smoothed Stats average away.
  history: Arc<Mutex<FrameHistory>>,
  // The engine-free frame protocol (demand gate, retained-list reuse, the
  // capture/destroy/content interlocks) lives in alloy; this bridge sequences
  // it and runs the JS hooks between the phases.
  driver: RefCell<FrameDriver>,
  // Whether an overlay display list is currently installed on the raster
  // thread (see Context::set_stats_overlay): drives the enable/disable edges
  // and the teardown clear in Drop.
  overlay_installed: Cell<bool>,
  // Last slow-frame warning: one line per second at most, so a sustained
  // storm reads as one warning per second in the logs, not one per frame.
  last_slow_warn: Cell<Option<Instant>>,
  // The window geometry the installed overlay was placed against: window
  // size, display scale, safe area. The overlay is positioned in window
  // space raster-side, so a geometry change refreshes it immediately rather
  // than waiting out the once-per-second cadence.
  overlay_key: Cell<(f32, f32, f32, f32, f32, f32, f32)>,
}

impl Drop for RenderInner {
  fn drop(&mut self) {
    // The overlay is this draw loop's state on the raster thread: clear it
    // with the engine, or an app switch leaves a stale HUD over the next
    // app's frames.
    if self.overlay_installed.get() {
      self.atx.set_stats_overlay(None);
    }
  }
}

/// Stash the draw bridge's host state in userdata, before any import. The
/// `srt:render` surface is registered separately via `module_override`.
pub fn store_state(
  ctx: &QuickJsContext<'_>,
  platform: Arc<PlatformContext>,
  atx: AlloyContext,
  input_state: Arc<InputState>,
  stats_snapshot: Arc<Mutex<stats::StatsSnapshot>>,
  history: Arc<Mutex<FrameHistory>>,
) {
  ctx
    .store_userdata(RenderState(Rc::new(RenderInner {
      platform,
      atx,
      input_state,
      stats_snapshot,
      stats: RefCell::new(stats::Stats::new()),
      history,
      driver: RefCell::new(FrameDriver::new()),
      overlay_installed: Cell::new(false),
      last_slow_warn: Cell::new(None),
      overlay_key: Cell::new((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)),
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

    // The stats overlay is its own demand source: its per-second figures keep
    // changing while the app is idle, so a due refresh forces a frame so the
    // HUD stays live, and an enable/disable edge forces one to show or clear
    // it. Refreshing costs no rebuild: the overlay is retained raster-side
    // and drawn over the finished frame (see Context::set_stats_overlay).
    // Latched before record_js below: its refresh() resets the same
    // once-per-second timer, so a read after it would never see a due
    // overlay.
    let stats_on = platform.stats_enabled();
    let win = platform.window_size();
    let sa = platform.safe_area();
    let overlay_key = (win.0, win.1, platform.display_scale(), sa.origin.x, sa.origin.y, sa.size.width, sa.size.height);
    let overlay_refresh = stats_on
      && (!self.overlay_installed.get() || self.overlay_key.get() != overlay_key || stats.borrow().overlay_due());
    let overlay_clear = !stats_on && self.overlay_installed.get();
    // The frame the runtime stamped for us (see frame::RenderFrame). Its
    // start is consumed here, so a native call with no render event (the
    // paused path) reads a zero JS cost instead of the stale stamp of the
    // last delivered frame; frame index and period stay for the record.
    let render_frame = crate::frame::RENDER_FRAME.with(|c| {
      let rf = c.get();
      c.set(crate::frame::RenderFrame { start: None, ..rf });
      rf
    });
    // JS render-handler cost (onFrame + flush), measured natively: time since
    // the instant stamped before the "render" event, now that the handler has
    // reached draw(). Recorded for every frame (gated ones too, since flush
    // still ran), with the FFI prop writes that flush produced.
    let js_ms = render_frame.start.map(|t| t.elapsed().as_secs_f32() * 1000.0).unwrap_or(0.0);
    let set_count = flux::gui::tree::SETPROP_COUNT.with(|c| c.replace(0));
    stats.borrow_mut().record_js(js_ms, set_count);
    // The figures as of this frame: published for out-of-loop readers on every
    // frame event, gated or not, so a query sees current numbers even while
    // the demand gate skips draws; and what the HUD below renders.
    let snap = stats.borrow().snapshot(render_frame.frame, platform.fps(), atx.textures.len());
    *stats_snapshot.lock().expect("stats snapshot lock poisoned") = snap;

    let tree = qtx.userdata::<flux::gui::tree::SharedRenderTree>().expect("render tree userdata");

    // Native transitions: advance every running track to this frame's
    // animation clock (stamped by the runtime before the frame's JS ran)
    // so the frame below paints the interpolated values. Runs before the
    // demand gate: the advance's damage is this frame's reason to rebuild.
    let anim_active = tree.0.borrow_mut().advance_transitions();

    // Demand-driven gate: when nothing requested a frame, skip it entirely
    // (layout, paint, submit, hover refresh - elements only move when a frame
    // is produced, so hover cannot have changed either). The overlay is the
    // bridge's own demand (see above); playback mode never gates. Running
    // transitions are demand too, and re-request below after `begin`
    // consumed the latch, so the loop keeps ticking until they settle.
    let mut driver = self.driver.borrow_mut();
    let Some(frame) = driver.begin(platform, overlay_refresh || overlay_clear || anim_active) else {
      stats.borrow_mut().note_skipped();
      return;
    };
    if anim_active {
      platform.request_frame();
    }

    // Push the overlay change ahead of this frame's submit (either path
    // below): the ordered raster channel then applies it to exactly this
    // frame. Built from the figures record_js just sampled; the raster
    // thread retains the list, so nothing is sent while the figures stand.
    if overlay_refresh {
      let overlay = overlay::build(&snap, &platform.typography(), platform.safe_area(), platform.display_scale());
      self.overlay_installed.set(overlay.is_some());
      self.overlay_key.set(overlay_key);
      atx.set_stats_overlay(overlay);
    } else if overlay_clear {
      atx.set_stats_overlay(None);
      self.overlay_installed.set(false);
    }

    // Content damage, then present-only reuse or the build handle: the
    // driver's interlocks (captures, deferred destroys, the window-shader
    // flush) run on whichever path resolves. On reuse, layout, postLayout and
    // hover refresh are skipped too - the tree and window are unchanged.
    let mut b = match frame
      .commit(&mut tree.0.borrow_mut(), platform, atx)
      .expect("Failed to submit display list")
    {
      Commit::Reused => {
        stats.borrow_mut().note_reused();
        return;
      }
      Commit::Build(b) => b,
    };

    let mut phases = stats::FramePhases::default();

    // Layout phase: the mut borrow is scoped to the call so onLayout handlers
    // (which may call setProperty etc.) don't trip the RefCell.
    let t = Instant::now();
    b.layout(&mut tree.0.borrow_mut(), platform, atx);
    phases.layout = t.elapsed();

    // Post-layout hook. Handlers run synchronously and may invalidate the
    // layout cache via setProperty; the paint phase re-runs layout to absorb
    // those changes.
    let t = Instant::now();
    emit_event(qtx, "postLayout", ());
    phases.post = t.elapsed();

    let t = Instant::now();
    let paint_stats = b.paint(&mut tree.0.borrow_mut(), platform, atx);
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
      let counters = rendertree::counters::take();
      s.record_layout_activity(tree.0.borrow().node_count(), counters);
      s.record_paint(paint_stats);
      let ms = |d: std::time::Duration| d.as_secs_f32() * 1000.0;
      let record = FrameRecord {
        at_ms: crate::frame_history::now_ms(),
        frame: render_frame.frame,
        period_ms: render_frame.period_ms,
        js_ms,
        layout_ms: ms(phases.layout),
        post_ms: ms(phases.post),
        paint_ms: ms(phases.paint),
        hover_ms: ms(phases.hover),
        total_ms: js_ms + ms(phases.layout + phases.post + phases.paint + phases.hover),
        counters,
        nodes_painted: paint_stats.nodes_painted,
        raster: atx.raster_counters(),
      };
      // A frame over its refresh period is jank a human feels; say so through
      // the engine logger (the one the dev server forwards, so get_logs sees
      // it) with the breakdown that names the phase.
      if record.total_ms > record.period_ms && record.period_ms > 0.0 {
        let due = self.last_slow_warn.get().is_none_or(|t| t.elapsed() >= SLOW_WARN_INTERVAL);
        if due {
          self.last_slow_warn.set(Some(Instant::now()));
          qtx.logger().warn(&format!(
            "Slow frame: {:.1} ms (budget {:.1}): js {:.1}, layout {:.1}, postLayout {:.1}, paint {:.1}, hover {:.1}; paraShapes {}, measureCalls {}, dirtiedNodes {}, cacheHits {}/{}, nodesPainted {}",
            record.total_ms,
            record.period_ms,
            record.js_ms,
            record.layout_ms,
            record.post_ms,
            record.paint_ms,
            record.hover_ms,
            counters.para_shapes,
            counters.measure_calls,
            counters.dirtied,
            counters.cache_hits,
            counters.cache_gets,
            paint_stats.nodes_painted,
          ));
        }
      }
      self.history.lock().expect("frame history lock poisoned").push(record);
    }

    b.finish(&tree.0.borrow(), platform, atx).expect("Failed to submit display list");
  }
}
