mod frame;
#[cfg(feature = "go")]
mod go;
mod overlay;
mod paced_clock;
mod plugins;
#[cfg(feature = "speech")]
pub mod speech;

#[cfg_attr(not(feature = "go"), allow(dead_code))]
enum EngineCmd {
  Stop,
  Reload(String),
}

use alloy::impellers::{ISize, Rect};
use flux::gui::AlloyContext;
use flux::{emit_event, ExecHandle, FluxEngine};
use frame::{EngineState, InputEvent, InputState};
use alloy::rendertree::{PlatformContext, RenderTree};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// --- Start Android entry point ------------------------------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn SDL_main(_argc: i32, _argv: *mut *mut i8) -> i32 {
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
  start(&rt, None, alloy::Mode::Run, (1280, 720), false);
  0
}

// Receives the soft-keyboard (IME) inset height in pixels from
// MainActivity.nativeKeyboardInset (Android UI thread) and stores it for the
// event loop to pick up. The export lives in the cdylib so the symbol lands in
// libmain.so; the env/class pointers are unused.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_solidrt_app_MainActivity_nativeKeyboardInset(
  _env: *mut core::ffi::c_void,
  _class: *mut core::ffi::c_void,
  px: core::ffi::c_int,
) {
  alloy::set_keyboard_inset_px(px as i32);
}

// --- End Android entry point ------------------------------

const DEFAULT_SOURCE: &str = include_str!("../default-app/app.srt.js");

pub(crate) const VERSION: &str = match option_env!("SOLIDRT_VERSION") {
  Some(v) => v,
  None => "0.0.0-dev",
};

/// QuickJS call-stack soft limit. Sits below the UI thread's native stack (see
/// alloy gl::run_context) so deep recursion throws a clean "Maximum call stack
/// size exceeded" instead of overflowing the OS stack. Tunable down per-app later.
const JS_STACK_SIZE: usize = 64 * 1024 * 1024;

/// The app to run: either JS source (dev/default) or precompiled bytecode (packed binary).
pub enum AppSource {
  Text(String),
  Bytecode(Vec<u8>),
}

fn emit_resize(eh: &ExecHandle, size: ISize, safe_area: Rect, display_scale: f32) {
  eh.exec(move |ctx| {
    // All four are insets: distance from the corresponding window edge, like CSS
    // env(safe-area-inset-*). safe_area is a rect in absolute coords, so the far
    // edges become (window extent - far edge).
    let sa = rquickjs::Object::new(ctx.clone()).expect("create safeArea");
    sa.set("top", safe_area.origin.y).expect("set top");
    sa.set("left", safe_area.origin.x).expect("set left");
    sa.set("right", size.width as f32 - (safe_area.origin.x + safe_area.size.width)).expect("set right");
    sa.set("bottom", size.height as f32 - (safe_area.origin.y + safe_area.size.height)).expect("set bottom");
    let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
    obj.set("width", size.width).expect("set width");
    obj.set("height", size.height).expect("set height");
    obj.set("safeArea", sa).expect("set safeArea");
    obj.set("displayScale", display_scale).expect("set displayScale");
    plugins::events::emit_sticky(&ctx, "resize", obj);
  });
}

/// Run the per-frame JS work for one frame signal (FrameRendered or idle
/// Tick): publish the frame index, advance the paced clock, pump cameras and
/// speech, flush rAF callbacks, and emit the "render" event. `next_frame` is
/// the present index the frame being computed would get.
fn emit_render_event(
  eh: &ExecHandle,
  next_frame: u64,
  record_frame: Arc<AtomicU64>,
  paced: Option<paced_clock::PacedClock>,
  platform: Arc<PlatformContext>,
) {
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
    plugins::speech::tick(&ctx);
    flux::gui::raf::flush(&ctx, ts);
    let time = ts / 1000.0;
    let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
    obj.set("frame", next_frame).expect("set frame");
    obj.set("time", time).expect("set time");
    // Stamp the start of the JS render handler so draw() can measure onFrame +
    // flush without any timing call crossing into JS (see frame::RENDER_START).
    crate::frame::RENDER_START.with(|c| c.set(Some(std::time::Instant::now())));
    emit_event(&ctx, "render", obj);
  });
}

/// Queue a pointer event for dispatch on the JS thread against the current
/// engine's tree. Drops the event when no engine is live (startup, mid-reload):
/// it was aimed at a tree that does not exist.
fn dispatch_input(
  current_exec: &Rc<RefCell<Option<ExecHandle>>>,
  current_engine_state: &Rc<RefCell<Option<Arc<EngineState>>>>,
  event: InputEvent,
) {
  let Some(es) = current_engine_state.borrow().as_ref().cloned() else {
    return;
  };
  if let Some(eh) = current_exec.borrow().as_ref() {
    eh.exec(move |ctx| plugins::input::dispatch(&ctx, event, &es));
  }
}

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  event_rx: std::sync::mpsc::Receiver<alloy::AlloyEvent>,
  app: Option<AppSource>,
  record_fps: Option<u32>,
  stats: bool,
) {
  // Anchor the process to a writable directory before any app code runs, so
  // relative paths (e.g. a flux:sqlite database) resolve to persistent storage.
  // The launch cwd is unreliable: on Android it is "/" (read-only); on desktop
  // it is wherever the client was spawned. SDL's pref path is writable and
  // persistent on every platform (and on Android is the same internal-storage
  // dir bundled assets are extracted into). The dev client and packed runtime
  // share one directory. The dev server is a separate process and unaffected.
  match alloy::sdl3::filesystem::get_pref_path("SolidRT", "go") {
    Ok(dir) => match std::env::set_current_dir(&dir) {
      Ok(()) => log::info!("[srt] working directory set to {}", dir.display()),
      Err(e) => log::warn!("[srt] could not set working directory to {}: {e}", dir.display()),
    },
    Err(e) => log::warn!("[srt] no writable pref path, leaving working directory unchanged: {e}"),
  }

  let platform = Arc::new(PlatformContext::new());
  // Record mode renders every frame unconditionally: the lockstep capture
  // loop blocks waiting for each frame's display list, so a frame skipped by
  // the demand-driven gate would deadlock it.
  platform.set_always_render(matches!(record_fps, Some(rfps) if rfps > 0));
  platform.set_stats_enabled(stats);
  let input_state = Arc::new(InputState::new());
  let mut current_app = app.unwrap_or_else(|| AppSource::Text(DEFAULT_SOURCE.to_string()));

  // Bridge the synchronous Alloy event channel onto an async one: a blocking
  // recv on a dedicated thread forwards each event, so the event loop can await
  // events instead of polling on a timer.
  let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<alloy::AlloyEvent>();
  std::thread::spawn(move || {
    while let Ok(event) = event_rx.recv() {
      if ev_tx.send(event).is_err() {
        break;
      }
    }
  });

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    let current_exec: Rc<RefCell<Option<ExecHandle>>> = Rc::new(RefCell::new(None));
    let current_exec_events = current_exec.clone();
    // Holds the active engine's state. Replaced on every reload, which
    // drops the previous EngineState (and any hover paths aimed at the
    // outgoing tree).
    let current_engine_state: Rc<RefCell<Option<Arc<EngineState>>>> = Rc::new(RefCell::new(None));
    let current_engine_state_events = current_engine_state.clone();

    let platform_events = platform.clone();
    let input_state_events = input_state.clone();
    // Virtual present counter the record-mode clock derives time from (frame/fps),
    // published by the FrameRendered handler. Unused in run mode.
    let record_frame = Arc::new(AtomicU64::new(0));
    let record_frame_events = record_frame.clone();
    // Run-mode pacing for the animation timestamps (see paced_clock). None in
    // record mode, which uses the deterministic frame/fps clock.
    let paced_clock_events = match record_fps {
      Some(rfps) if rfps > 0 => None,
      _ => Some(paced_clock::PacedClock::new()),
    };
    local.spawn_local(async move {
      while let Some(event) = ev_rx.recv().await {
        match event {
          alloy::AlloyEvent::Quit => std::process::exit(0),
          alloy::AlloyEvent::WindowFocus => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                emit_event(&ctx, "windowFocus", obj);
              });
            }
          }
          alloy::AlloyEvent::WindowBlur => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                emit_event(&ctx, "windowBlur", obj);
              });
            }
          }
          alloy::AlloyEvent::Resize { size, safe_area, display_scale } => {
            platform_events.set_window_size(size.width as f32, size.height as f32);
            platform_events.set_display_scale(display_scale);
            platform_events.set_safe_area(safe_area);
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              emit_resize(eh, size, safe_area, display_scale);
            }
          }
          // Pointer events dispatch on arrival (hit test against the last
          // computed layout, like Flutter): no frame is needed to deliver
          // them. Handlers that mutate state request the next frame through
          // their ffi calls.
          alloy::AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => {
            input_state_events.set_pointer_pos((pointer_type, pointer_id), x, y);
            input_state_events.set_modifiers(modifiers);
            dispatch_input(
              &current_exec_events,
              &current_engine_state_events,
              InputEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers },
            );
          }
          alloy::AlloyEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
            input_state_events.set_pointer_pos((pointer_type, pointer_id), x, y);
            input_state_events.set_modifiers(modifiers);
            dispatch_input(
              &current_exec_events,
              &current_engine_state_events,
              InputEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers },
            );
          }
          alloy::AlloyEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
            input_state_events.set_pointer_pos((pointer_type, pointer_id), x, y);
            input_state_events.set_modifiers(modifiers);
            // Touch pointers end at release; mouse pointers persist.
            if pointer_type == alloy::PointerType::Touch {
              input_state_events.remove_pointer((pointer_type, pointer_id));
            }
            dispatch_input(
              &current_exec_events,
              &current_engine_state_events,
              InputEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers },
            );
          }
          alloy::AlloyEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
            input_state_events.set_modifiers(modifiers);
            dispatch_input(
              &current_exec_events,
              &current_engine_state_events,
              InputEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers },
            );
          }
          alloy::AlloyEvent::KeyDown { keycode, scancode, modifiers } => {
            input_state_events.set_modifiers(modifiers);
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              let key = keycode.map(|k| k.name()).unwrap_or_default();
              let code = scancode.map(|s| s.name().to_string()).unwrap_or_default();
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("key", key).expect("set key");
                obj.set("code", code).expect("set code");
                obj.set("shiftKey", modifiers.shift).expect("set shiftKey");
                obj.set("ctrlKey", modifiers.ctrl).expect("set ctrlKey");
                obj.set("altKey", modifiers.alt).expect("set altKey");
                obj.set("metaKey", modifiers.meta).expect("set metaKey");
                emit_event(&ctx, "keydown", obj);
              });
            }
          }
          alloy::AlloyEvent::KeyUp { keycode, scancode, modifiers } => {
            input_state_events.set_modifiers(modifiers);
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              let key = keycode.map(|k| k.name()).unwrap_or_default();
              let code = scancode.map(|s| s.name().to_string()).unwrap_or_default();
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("key", key).expect("set key");
                obj.set("code", code).expect("set code");
                obj.set("shiftKey", modifiers.shift).expect("set shiftKey");
                obj.set("ctrlKey", modifiers.ctrl).expect("set ctrlKey");
                obj.set("altKey", modifiers.alt).expect("set altKey");
                obj.set("metaKey", modifiers.meta).expect("set metaKey");
                emit_event(&ctx, "keyup", obj);
              });
            }
          }
          alloy::AlloyEvent::TextInput { text } => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("text", text).expect("set text");
                emit_event(&ctx, "textInput", obj);
              });
            }
          }
          alloy::AlloyEvent::KeyboardVisibility { shown, height } => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("shown", shown).expect("set shown");
                obj.set("height", height).expect("set height");
                emit_event(&ctx, "keyboardVisibility", obj);
              });
            }
          }
          alloy::AlloyEvent::CameraDeviceChange { added } => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("added", added).expect("set added");
                emit_event(&ctx, "cameraDeviceChange", obj);
              });
            }
          }
          alloy::AlloyEvent::PowerStatus { info } => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              use alloy::sdl_utils::PowerState;
              let state = match info.state {
                PowerState::OnBattery => "onBattery",
                PowerState::Charging => "charging",
                PowerState::Charged => "charged",
                PowerState::NoBattery => "noBattery",
                PowerState::Unknown => "unknown",
              };
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("state", state).expect("set state");
                match info.percent {
                  Some(p) => obj.set("percent", p).expect("set percent"),
                  None => obj.set("percent", rquickjs::Null).expect("set percent null"),
                }
                emit_event(&ctx, "powerStatus", obj);
              });
            }
          }
          alloy::AlloyEvent::FrameRendered { frame, fps, time: _ } => {
            platform_events.set_fps(fps);
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              // FrameRendered reports the frame native just finished
              // drawing. JS uses the "render" event to compute the NEXT
              // frame's state, so shift the field by +1. The JS-side
              // bootstrap owns frame 0; without the shift, record mode
              // re-runs frame 0 at tick 0 and duplicates a PNG.
              emit_render_event(
                eh,
                frame + 1,
                record_frame_events.clone(),
                paced_clock_events.clone(),
                platform_events.clone(),
              );
            }
          }
          alloy::AlloyEvent::Tick { frame, fps } => {
            platform_events.set_fps(fps);
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              // Tick's frame is already the next present index (one past the
              // last FrameRendered), so no +1 here.
              emit_render_event(
                eh,
                frame,
                record_frame_events.clone(),
                paced_clock_events.clone(),
                platform_events.clone(),
              );
            }
          }
          alloy::AlloyEvent::DisplayRefreshRate { hz } => {
            if let Some(pc) = &paced_clock_events {
              pc.set_hz(hz);
            }
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("hz", hz).expect("set hz");
                plugins::events::emit_sticky(&ctx, "displayRefreshRate", obj);
              });
            }
          }
        }
      }
    });

    #[cfg_attr(not(feature = "go"), allow(unused_variables))]
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<EngineCmd>();
    // The dev-server client: connection supervisor, recents, proxy state and the
    // srt.dev surface. None in record mode (and entirely absent without the
    // `go` feature). This is the runtime's only seam to the dev client.
    #[cfg(feature = "go")]
    let dev_session = go::DevSession::start(
      &handle,
      cmd_tx.clone(),
      record_fps,
      &local,
      current_exec.clone(),
      platform.stats_handles(),
    );

    // flux::Clock backs performance.now() (and the run-mode paced clock corrects
    // toward it). Injected into each engine; persists across reloads for continuous
    // time.
    let clock = match record_fps {
      // Record mode: derive time from the present counter (frame/fps) so the
      // whole JS time surface is deterministic and recordings reproducible.
      Some(rfps) if rfps > 0 => {
        let record_frame = record_frame.clone();
        flux::Clock::new(move || record_frame.load(Ordering::Relaxed) as f64 * 1000.0 / rfps as f64)
      }
      // Run mode: wall clock built on tokio's Instant so the time surface stays
      // controllable under tokio's test clock (pause / advance).
      _ => {
        let raf_start = tokio::time::Instant::now();
        flux::Clock::new(move || raf_start.elapsed().as_secs_f64() * 1000.0)
      }
    };

    loop {
      let render_tree = RenderTree::new();
      let platform = platform.clone();
      let atx = atx.clone();
      // A reloaded app must not inherit (or leak) the previous app's open
      // capture devices; their JS handles died with the old engine.
      atx.close_all_cameras();
      atx.close_all_microphones();
      let input_state = input_state.clone();
      let engine_state = Arc::new(EngineState::new());
      *current_engine_state.borrow_mut() = Some(engine_state.clone());

      let draw_platform = platform.clone();
      let draw_atx = atx.clone();
      #[cfg(feature = "speech")]
      let speech_atx = AlloyContext(atx.clone());
      let builder = FluxEngine::builder()
        .stack_size(JS_STACK_SIZE)
        .logger(|level, msg| match level {
          flux::LogLevel::Debug => log::debug!("{msg}"),
          flux::LogLevel::Log => log::info!("{msg}"),
          flux::LogLevel::Warn => log::warn!("{msg}"),
          flux::LogLevel::Error => log::error!("{msg}"),
        });
      // flux owns the gui plugin set and its registration order (the tree plugin
      // creates the `ffi` global the draw bridge attaches to, so install runs
      // before draw); lattice only supplies the host instances they bind.
      let builder = flux::gui::install(
        builder,
        flux::gui::GuiHost { platform: platform.clone(), alloy: atx.clone(), render_tree, alloy_cmd_tx: alloy_cmd_tx.clone() },
      );
      let builder = builder
        .plugin(move |ctx| plugins::draw::init(ctx, draw_platform, AlloyContext(draw_atx), input_state, engine_state))
        .plugin(|ctx| plugins::image::init(ctx))
        .plugin(|ctx| plugins::events::init(&ctx))
        .module_override("srt:events", plugins::events::SrtEventsModule)
        .module_override("srt:dev", plugins::dev::SrtDevModule)
        .userdata(clock.clone());
      #[cfg(feature = "speech")]
      let builder = builder.plugin(move |ctx| plugins::speech::init(ctx, speech_atx));
      // Install the dev-server control surface and (when enabled) the proxy.
      #[cfg(feature = "go")]
      let builder = match &dev_session {
        Some(dev) => dev.augment_builder(builder),
        None => builder,
      };
      let engine = builder.build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();
      // Replay the current connection state into this engine so a reload (e.g.
      // a server stop returning to the default app) keeps the right indicator.
      #[cfg(feature = "go")]
      if let Some(dev) = &dev_session {
        dev.replay_state(&engine.exec_handle());
      }

      log::info!("[srt] flux engine start");
      let mut next_app: Option<AppSource> = None;
      local
        .run_until(async {
          tokio::select! {
            _ = async {
              match &current_app {
                AppSource::Text(src) => engine.eval_source(src).await,
                AppSource::Bytecode(bytes) => engine.eval(bytes.clone()).await,
              }
            } => {}
            Some(cmd) = cmd_rx.recv() => {
              match cmd {
                EngineCmd::Reload(src) => { next_app = Some(AppSource::Text(src)); }
                EngineCmd::Stop => { next_app = Some(AppSource::Text(DEFAULT_SOURCE.to_string())); }
              }
            }
          }
        })
        .await;
      if let Some(app) = next_app {
        current_app = app;
      }
    }
  });
}

pub fn start(rt: &tokio::runtime::Runtime, app_source: Option<AppSource>, mode: alloy::Mode, size: (u32, u32), stats: bool) {
  alloy::install_logger();
  log::info!("[srt] SolidRT version {VERSION}");

  let handle = rt.handle().clone();
  let record_fps = match &mode {
    alloy::Mode::Record(record) => Some(record.fps),
    _ => None,
  };
  let app = alloy::setup("SolidRT", ISize::new(size.0 as i64, size.1 as i64), mode);

  app.run(move |atx, alloy_cmd_tx, event_rx| {
    ui_thread(handle, atx, alloy_cmd_tx, event_rx, app_source, record_fps, stats);
  });
}
