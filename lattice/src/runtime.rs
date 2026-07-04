use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use alloy::rendertree::PlatformContext;
use alloy::AlloyEvent;
use flux::gui::input::InputEvent;
use flux::{emit_event, ExecHandle};

use crate::paced_clock::PacedClock;

/// The engine seam: the verbs the runner's event loop drives a UI runtime
/// with. Runner-owned bookkeeping (pointer positions, window facts, fps) has
/// already been applied when a verb is called; implementations only marshal
/// into their engine. Engine lifecycle (build, eval, reload) is deliberately
/// not part of the contract yet: its shape depends on how the engine's async
/// loop and a verb-driven loop reconcile (see REDESIGN.md), and freezing a
/// guess here would break the contract later.
pub trait UiRuntime {
  /// An alloy event arrived. Frame signals (FrameRendered / Tick) never come
  /// through here; they become `frame` calls.
  fn event(&mut self, event: &AlloyEvent);
  /// A frame signal (present or idle tick): run the engine's per-frame work
  /// computing frame `next_frame`.
  fn frame(&mut self, next_frame: u64);
}

/// Drives a flux (QuickJS) engine: events are marshalled onto the JS thread
/// via the engine's ExecHandle. The handle cell is shared with the reload
/// loop, which swaps in each new engine's handle; while no engine is live
/// (startup, mid-reload) both verbs drop their input - it was aimed at an
/// engine that does not exist.
pub struct FluxRuntime {
  exec: Rc<RefCell<Option<ExecHandle>>>,
  // Virtual present counter the record-mode clock derives time from,
  // published by frame(). Unused in run mode.
  record_frame: Arc<AtomicU64>,
  // Run-mode pacing for the animation timestamps (see paced_clock). None in
  // record mode, which uses the deterministic frame/fps clock.
  paced: Option<PacedClock>,
  platform: Arc<PlatformContext>,
}

impl FluxRuntime {
  pub fn new(
    exec: Rc<RefCell<Option<ExecHandle>>>,
    record_frame: Arc<AtomicU64>,
    paced: Option<PacedClock>,
    platform: Arc<PlatformContext>,
  ) -> Self {
    Self { exec, record_frame, paced, platform }
  }
}

impl UiRuntime for FluxRuntime {
  fn event(&mut self, event: &AlloyEvent) {
    // The pacing clock's refresh rate is this runtime's own bookkeeping (its
    // tick runs inside frame()), tracked whether or not an engine is live.
    if let AlloyEvent::DisplayRefreshRate { hz } = event {
      if let Some(pc) = &self.paced {
        pc.set_hz(*hz);
      }
    }
    let exec = self.exec.borrow();
    let Some(eh) = exec.as_ref() else {
      return;
    };
    // flux marshals the engine-agnostic window / keyboard / device events
    // (including the sticky window facts) directly; pointer events remain
    // because their dispatch is hit-testing, not pure marshalling.
    if flux::gui::events::forward(eh, event) {
      return;
    }
    match event {
      // Pointer events dispatch on arrival (hit test against the last
      // computed layout, like Flutter): no frame is needed to deliver them.
      // Handlers that mutate state request the next frame through their ffi
      // calls.
      AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => dispatch(
        eh,
        InputEvent::PointerMove {
          pointer_id: *pointer_id,
          pointer_type: *pointer_type,
          x: *x,
          y: *y,
          modifiers: *modifiers,
        },
      ),
      AlloyEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => dispatch(
        eh,
        InputEvent::PointerDown {
          pointer_id: *pointer_id,
          pointer_type: *pointer_type,
          button: *button,
          x: *x,
          y: *y,
          modifiers: *modifiers,
        },
      ),
      AlloyEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => dispatch(
        eh,
        InputEvent::PointerUp {
          pointer_id: *pointer_id,
          pointer_type: *pointer_type,
          button: *button,
          x: *x,
          y: *y,
          modifiers: *modifiers,
        },
      ),
      AlloyEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => dispatch(
        eh,
        InputEvent::Wheel {
          pointer_id: *pointer_id,
          pointer_type: *pointer_type,
          x: *x,
          y: *y,
          delta_x: *delta_x,
          delta_y: *delta_y,
          modifiers: *modifiers,
        },
      ),
      _ => {}
    }
  }

  /// Run the per-frame JS work for one frame signal (FrameRendered or idle
  /// Tick): publish the frame index, advance the paced clock, pump cameras and
  /// speech, flush rAF callbacks, and emit the "render" event. `next_frame` is
  /// the present index the frame being computed would get.
  fn frame(&mut self, next_frame: u64) {
    let exec = self.exec.borrow();
    let Some(eh) = exec.as_ref() else {
      return;
    };
    let record_frame = self.record_frame.clone();
    let paced = self.paced.clone();
    let platform = self.platform.clone();
    eh.exec(move |ctx| {
      // Publish the present being computed before reading the clock, so in
      // record mode the clock reports this frame's virtual time.
      record_frame.store(next_frame, Ordering::Relaxed);
      // rAF and the render event use the paced clock in run mode (see
      // paced_clock); record mode and performance.now() read flux::Clock
      // directly. Idle Ticks arrive at the refresh cadence, so ticking the
      // paced clock for them preserves its one-period-per-call model. Render
      // event carries seconds; JS scales to ms.
      let raw = ctx.userdata::<flux::Clock>().map(|c| c.now_ms()).unwrap_or(0.0);
      let ts = match &paced {
        Some(pc) => {
          pc.tick(raw);
          pc.now_ms()
        }
        None => raw,
      };
      if flux::gui::camera::tick(&ctx) {
        // A camera frame landed in its texture; the screen content changed
        // even though the tree did not.
        platform.request_frame();
      }
      #[cfg(feature = "speech")]
      crate::plugins::speech::tick(&ctx);
      flux::gui::raf::flush(&ctx, ts);
      let time = ts / 1000.0;
      let obj = flux::rquickjs::Object::new(ctx.clone()).expect("create object");
      obj.set("frame", next_frame).expect("set frame");
      obj.set("time", time).expect("set time");
      // Stamp the start of the JS render handler so draw() can measure onFrame +
      // flush without any timing call crossing into JS (see frame::RENDER_START).
      crate::frame::RENDER_START.with(|c| c.set(Some(std::time::Instant::now())));
      emit_event(&ctx, "render", obj);
    });
  }
}

// Queue a pointer event for hit-test dispatch on the JS thread (see
// flux::gui::input::dispatch).
fn dispatch(eh: &ExecHandle, event: InputEvent) {
  eh.exec(move |ctx| flux::gui::input::dispatch(&ctx, event));
}
