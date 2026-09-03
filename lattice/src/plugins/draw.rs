use crate::frame_history::{FrameHistory, FrameRecord};
use crate::overlay;
use crate::stats;
use alloy::InputState;
use alloy::rendertree::{self, PlatformContext};
use flux::{
  emit_event,
  gui::frame::Commit,
  rquickjs::{
    module::{Declarations, Exports, ModuleDef},
    Ctx as QuickJsContext, Function, JsLifetime,
  },
  CtxLogger,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// Slow-frame warnings are throttled to one per this interval.
const SLOW_WARN_INTERVAL: Duration = Duration::from_secs(1);

// The host state the `srt:render` module binds, stashed in userdata by
// `store_state` before any import so the module's `evaluate` can build
// `renderFrame`. Also holds the draw loop's own frame-to-frame state (stats,
// overlay) so the body is callable both from the JS export and natively
// (see `render_now`). The tree it draws and the frame protocol over it are
// flux's (`frame::draw`).
#[derive(Clone, JsLifetime)]
struct RenderState(#[qjs(skip_trace)] Rc<RenderInner>);

struct RenderInner {
  platform: Arc<PlatformContext>,
  atx: Arc<alloy::Context>,
  input_state: Arc<InputState>,
  // Latest stats figures, published every frame for readers outside the draw
  // loop (the dev server's stats query answers from here).
  stats_snapshot: Arc<Mutex<stats::StatsSnapshot>>,
  stats: RefCell<stats::Stats>,
  // Raw per-rebuild records for the stats query's window summary (worst
  // frame, percentiles), the figures the smoothed Stats average away.
  history: Arc<Mutex<FrameHistory>>,
  // The dev-session facts the overlay's badge shows (see overlay::Badge):
  // written by the dev connection (go/connection.rs), which latches a frame
  // request on every edge so the change is drawn on an idle app.
  dev_connected: Arc<AtomicBool>,
  user_input_muted: Arc<AtomicBool>,
  // Whether an overlay display list is currently installed on the raster
  // thread (see Context::set_overlay): drives the enable/disable edges
  // and the teardown clear in Drop.
  overlay_installed: Cell<bool>,
  // Last slow-frame warning: one line per second at most, so a sustained
  // storm reads as one warning per second in the logs, not one per frame.
  last_slow_warn: Cell<Option<Instant>>,
  // Node count at the previous frame: the delta is what a slow-frame line
  // reports as nodesAdded, so a mount frame's honest build cost can be told
  // from steady-state jank.
  last_node_count: Cell<usize>,
  // What the installed overlay was built against: window geometry (size,
  // display scale, safe area - the overlay is positioned in window space
  // raster-side) and what it shows (HUD on, badge). A change refreshes it
  // immediately rather than waiting out the once-per-second cadence.
  overlay_key: Cell<OverlayKey>,
}

type OverlayKey = (f32, f32, f32, f32, f32, f32, f32, bool, Option<overlay::Badge>);

impl Drop for RenderInner {
  fn drop(&mut self) {
    // The overlay is this draw loop's state on the raster thread: clear it
    // with the engine, or an app switch leaves a stale HUD over the next
    // app's frames.
    if self.overlay_installed.get() {
      self.atx.set_overlay(None);
    }
  }
}

/// Stash the draw bridge's host state in userdata, before any import. The
/// `srt:render` surface is registered separately via `module_override`.
pub fn store_state(
  ctx: &QuickJsContext<'_>,
  platform: Arc<PlatformContext>,
  atx: Arc<alloy::Context>,
  input_state: Arc<InputState>,
  stats_snapshot: Arc<Mutex<stats::StatsSnapshot>>,
  history: Arc<Mutex<FrameHistory>>,
  dev_connected: Arc<AtomicBool>,
  user_input_muted: Arc<AtomicBool>,
) {
  ctx
    .store_userdata(RenderState(Rc::new(RenderInner {
      platform,
      atx,
      input_state,
      stats_snapshot,
      stats: RefCell::new(stats::Stats::new()),
      history,
      dev_connected,
      user_input_muted,
      overlay_installed: Cell::new(false),
      last_slow_warn: Cell::new(None),
      last_node_count: Cell::new(0),
      overlay_key: Cell::new((0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, false, None)),
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
/// part of `flux:rendertree` because it bundles lattice-only policy (the
/// stats overlay and figures, the frame history, hover refresh, the
/// postLayout hook) around flux's frame protocol.
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

    // The overlay (the dev-session badge, the stats HUD when toggled on) is
    // its own demand source: its per-second figures keep changing while the
    // app is idle, so a due refresh forces a frame so the line stays live,
    // and an edge (show/clear, badge change) forces one to draw it.
    // Refreshing costs no rebuild: the overlay is retained raster-side and
    // drawn over the finished frame (see Context::set_overlay). Latched
    // before record_js below: its refresh() resets the same once-per-second
    // timer, so a read after it would never see a due overlay.
    let stats_on = platform.stats_enabled();
    let badge = if self.user_input_muted.load(Ordering::Relaxed) {
      Some(overlay::Badge::Muted)
    } else if self.dev_connected.load(Ordering::Relaxed) {
      Some(overlay::Badge::Connected)
    } else {
      None
    };
    let overlay_on = stats_on || badge.is_some();
    let win = platform.window_size();
    let sa = platform.safe_area();
    let overlay_key: OverlayKey = (
      win.0,
      win.1,
      platform.display_scale(),
      sa.origin.x,
      sa.origin.y,
      sa.size.width,
      sa.size.height,
      stats_on,
      badge,
    );
    let overlay_refresh = overlay_on
      && (!self.overlay_installed.get() || self.overlay_key.get() != overlay_key || stats.borrow().overlay_due());
    let overlay_clear = !overlay_on && self.overlay_installed.get();
    // The frame the runtime stamped for us (see frame::RenderFrame). Its
    // start is consumed here, so a native call with no render event (the
    // paused path) reads a zero JS cost instead of the stale stamp of the
    // last delivered frame; frame index and period stay for the record.
    let render_frame = crate::frame::RENDER_FRAME.with(|c| {
      let rf = c.get();
      c.set(crate::frame::RenderFrame { start: None, ..rf });
      rf
    });
    // The frame's JS cost (timers, rAF, onFrame + flush), measured natively:
    // time since the instant stamped before the frame was delivered, now
    // that the render handler has reached draw(). Recorded for every frame
    // (gated ones too, since flush still ran), with the FFI prop writes that
    // flush produced.
    let js_ms = render_frame.start.map(|t| t.elapsed().as_secs_f32() * 1000.0).unwrap_or(0.0);
    let set_count = flux::gui::tree::SETPROP_COUNT.with(|c| c.replace(0));
    stats.borrow_mut().record_gpu(render_frame.frame, &atx.raster_counters());
    stats.borrow_mut().record_js(js_ms, set_count);
    // The figures as of this frame: published for out-of-loop readers on every
    // frame event, gated or not, so a query sees current numbers even while
    // the demand gate skips draws; and what the HUD below renders.
    let snap = stats.borrow().snapshot(render_frame.frame, platform.fps(), atx.textures.len());
    *stats_snapshot.lock().expect("stats snapshot lock poisoned") = snap;

    // The frame protocol - the transition ticks (render tree, spatial arena,
    // each settling to JS before the frame paints), the demand gate, reuse
    // or rebuild - is flux's `frame::draw`; this bridge supplies its own
    // demand and runs its policy between the phases. The overlay is that
    // demand (see above); playback mode never gates.
    flux::gui::frame::draw(qtx, overlay_refresh || overlay_clear, |frame| {
      // Demand-driven gate: when nothing requested a frame, skip it entirely
      // (layout, paint, submit, hover refresh - elements only move when a
      // frame is produced, so hover cannot have changed either).
      let Some(frame) = frame else {
        let mut s = stats.borrow_mut();
        s.note_skipped();
        s.record_frame(stats::FramePhases::default());
        s.record_paint(rendertree::composite::PaintStats::default());
        return;
      };

      // Push the overlay change ahead of this frame's submit (either path
      // below): the ordered raster channel then applies it to exactly this
      // frame. Built from the figures record_js just sampled; the raster
      // thread retains the list, so nothing is sent while the figures stand.
      if overlay_refresh {
        let overlay = overlay::build(
          &snap,
          stats_on,
          badge,
          &platform.typography(),
          platform.safe_area(),
          platform.display_scale(),
        );
        self.overlay_installed.set(overlay.is_some());
        self.overlay_key.set(overlay_key);
        atx.set_overlay(overlay);
      } else if overlay_clear {
        atx.set_overlay(None);
        self.overlay_installed.set(false);
      }

      // Content damage, then present-only reuse or the build handle: the
      // driver's interlocks (captures, deferred destroys, the window-shader
      // flush) run on whichever path resolves. On reuse, layout, postLayout and
      // hover refresh are skipped too - the tree and window are unchanged. The
      // phases and paint counts are still recorded, as zero: the skip path
      // above does the same, so the smoothed phase figures track every frame
      // the JS thread sees and decay when nothing rebuilds, and the paint walk
      // counts describe this frame (no walk) rather than the last one that had
      // one. A tree that never rebuilds otherwise presents one stale rebuild's
      // cost as a live share of a moving frame period (an app reusing its
      // display list at 60 Hz showed PNT 350%) and its boundary counts as
      // current.
      let mut b = match frame.commit().expect("Failed to submit display list") {
        Commit::Reused { content_changed } => {
          let mut s = stats.borrow_mut();
          s.note_reused();
          s.record_frame(stats::FramePhases::default());
          s.record_paint(rendertree::composite::PaintStats::default());
          // GPU content presented through the reused display list (a layer
          // write, a shader param, an upload) still changed the picture - a
          // sprite app's every frame is one. Record it, with the render
          // handler as its whole critical path, or the frame window reads
          // `frames: 0` for exactly the apps that animate every frame.
          if content_changed {
            self.history.lock().expect("frame history lock poisoned").push(FrameRecord {
              at_ms: crate::frame_history::now_ms(),
              frame: render_frame.frame,
              period_ms: render_frame.period_ms,
              js_ms,
              total_ms: js_ms,
              raster: atx.raster_counters(),
              ..FrameRecord::default()
            });
          }
          return;
        }
        Commit::Build(b) => b,
      };

      let mut phases = stats::FramePhases::default();

      // Layout phase: the tree borrow is scoped to the call so onLayout
      // handlers (which may call setProperty etc.) don't trip the RefCell.
      let t = Instant::now();
      b.layout();
      phases.layout = t.elapsed();

      // Post-layout hook. Handlers run synchronously and may invalidate the
      // layout cache via setProperty; the paint phase re-runs layout to absorb
      // those changes.
      let t = Instant::now();
      emit_event(qtx, "postLayout", ());
      phases.post = t.elapsed();

      let t = Instant::now();
      let paint_stats = b.paint();
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
        let nodes = flux::gui::tree::node_counts(qtx).map_or(0, |(_, total)| total);
        let nodes_added = nodes.saturating_sub(self.last_node_count.replace(nodes));
        s.record_layout_activity(nodes, counters);
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
              "Slow frame: {:.1} ms (budget {:.1}): js {:.1}, layout {:.1}, postLayout {:.1}, paint {:.1}, hover {:.1}; paraShapes {}, measureCalls {}, dirtiedNodes {}, nodesAdded {}, cacheHits {}/{}, nodesPainted {}",
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
              nodes_added,
              counters.cache_hits,
              counters.cache_gets,
              paint_stats.nodes_painted,
            ));
          }
        }
        self.history.lock().expect("frame history lock poisoned").push(record);
      }

      b.finish().expect("Failed to submit display list");
    });
  }
}
