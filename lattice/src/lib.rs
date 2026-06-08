mod frame;
#[cfg(feature = "go")]
mod go;
mod overlay;
mod paced_clock;
mod plugins;
mod rendertree;

#[cfg_attr(not(feature = "go"), allow(dead_code))]
enum EngineCmd {
  Stop,
  Reload(String),
}

use alloy::impellers::{ISize, Rect};
use flux::rquickjs::JsLifetime;
use flux::{emit_event, ExecHandle, FluxEngine};
use frame::{EngineState, InputEvent, InputState};
use rendertree::{PlatformContext, RenderTree};
use std::cell::RefCell;
use std::rc::Rc;
#[cfg(feature = "go")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// --- Start Android entry point ------------------------------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn SDL_main(_argc: i32, _argv: *mut *mut i8) -> i32 {
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
  start(&rt, None, alloy::Mode::Run, (1280, 720));
  0
}

// --- End Android entry point ------------------------------

#[derive(Clone, JsLifetime)]
pub(crate) struct AlloyContext(#[qjs(skip_trace)] pub(crate) Arc<alloy::Context>);

impl std::ops::Deref for AlloyContext {
  type Target = alloy::Context;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

const DEFAULT_SOURCE: &str = include_str!("../default-app/app.srt.js");

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
    let sa = rquickjs::Object::new(ctx.clone()).expect("create safeArea");
    sa.set("top", safe_area.origin.y).expect("set top");
    sa.set("left", safe_area.origin.x).expect("set left");
    sa.set("right", safe_area.origin.x + safe_area.size.width).expect("set right");
    sa.set("bottom", safe_area.origin.y + safe_area.size.height).expect("set bottom");
    let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
    obj.set("width", size.width).expect("set width");
    obj.set("height", size.height).expect("set height");
    obj.set("safeArea", sa).expect("set safeArea");
    obj.set("displayScale", display_scale).expect("set displayScale");
    plugins::events::emit_sticky(&ctx, "resize", obj);
  });
}

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  event_rx: std::sync::mpsc::Receiver<alloy::AlloyEvent>,
  app: Option<AppSource>,
  record_fps: Option<u32>,
) {
  #[cfg(feature = "go")]
  let proxy_files_enabled = Arc::new(AtomicBool::new(false));
  #[cfg(feature = "go")]
  let proxy_http_enabled = Arc::new(AtomicBool::new(false));
  let platform = Arc::new(PlatformContext::new());
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
    // drops the previous EngineState (and any queued input aimed at the
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
          alloy::AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers } => {
            input_state_events.set_pointer_pos((pointer_type, pointer_id), x, y);
            input_state_events.set_modifiers(modifiers);
            if let Some(es) = current_engine_state_events.borrow().as_ref() {
              es.push_input(InputEvent::PointerMove { pointer_id, pointer_type, x, y, modifiers });
            }
          }
          alloy::AlloyEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers } => {
            input_state_events.set_pointer_pos((pointer_type, pointer_id), x, y);
            input_state_events.set_modifiers(modifiers);
            if let Some(es) = current_engine_state_events.borrow().as_ref() {
              es.push_input(InputEvent::PointerDown { pointer_id, pointer_type, button, x, y, modifiers });
            }
          }
          alloy::AlloyEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers } => {
            input_state_events.set_pointer_pos((pointer_type, pointer_id), x, y);
            input_state_events.set_modifiers(modifiers);
            if let Some(es) = current_engine_state_events.borrow().as_ref() {
              es.push_input(InputEvent::PointerUp { pointer_id, pointer_type, button, x, y, modifiers });
            }
            // Touch pointers end at release; mouse pointers persist.
            if pointer_type == alloy::PointerType::Touch {
              input_state_events.remove_pointer((pointer_type, pointer_id));
            }
          }
          alloy::AlloyEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers } => {
            input_state_events.set_modifiers(modifiers);
            if let Some(es) = current_engine_state_events.borrow().as_ref() {
              es.push_input(InputEvent::Wheel { pointer_id, pointer_type, x, y, delta_x, delta_y, modifiers });
            }
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
          alloy::AlloyEvent::KeyboardVisibility { shown } => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("shown", shown).expect("set shown");
                emit_event(&ctx, "keyboardVisibility", obj);
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
              // frame's state, so shift both fields by +1. The JS-side
              // bootstrap owns frame 0; without the shift, record mode
              // re-runs frame 0 at tick 0 and duplicates a PNG.
              let next_frame = frame + 1;
              let record_frame = record_frame_events.clone();
              let paced = paced_clock_events.clone();
              eh.exec(move |ctx| {
                // Publish the present being computed before reading the clock, so
                // in record mode the clock reports this frame's virtual time.
                record_frame.store(next_frame as u64, Ordering::Relaxed);
                // rAF and the render event use the paced clock in run mode (see
                // paced_clock); record mode and performance.now() read flux::Clock
                // directly. Render event carries seconds; JS scales to ms.
                let raw = ctx.userdata::<flux::Clock>().map(|c| c.now_ms()).unwrap_or(0.0);
                let ts = match &paced {
                  Some(pc) => {
                    pc.tick(raw);
                    pc.now_ms()
                  }
                  None => raw,
                };
                plugins::raf::flush(&ctx, ts);
                let time = ts / 1000.0;
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("frame", next_frame).expect("set frame");
                obj.set("time", time).expect("set time");
                emit_event(&ctx, "render", obj);
              });
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
    #[cfg(feature = "go")]
    let dev_server: go::DevServerCell = std::sync::Arc::new(tokio::sync::OnceCell::new());
    #[cfg(feature = "go")]
    if record_fps.is_none() {
      go::start(&handle, cmd_tx.clone(), dev_server.clone(), proxy_files_enabled.clone(), proxy_http_enabled.clone());
    }

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
      let input_state = input_state.clone();
      let engine_state = Arc::new(EngineState::new());
      *current_engine_state.borrow_mut() = Some(engine_state.clone());

      let tree_cmd_tx = alloy_cmd_tx.clone();
      let tree_platform = platform.clone();
      let tree_atx = AlloyContext(atx.clone());
      let texture_atx = AlloyContext(atx.clone());
      #[cfg_attr(not(feature = "go"), allow(unused_mut))]
      let mut builder = FluxEngine::builder()
        .stack_size(JS_STACK_SIZE)
        .logger(|level, msg| match level {
          flux::LogLevel::Debug => log::debug!("{msg}"),
          flux::LogLevel::Log => log::info!("{msg}"),
          flux::LogLevel::Warn => log::warn!("{msg}"),
          flux::LogLevel::Error => log::error!("{msg}"),
        })
        .plugin(move |ctx| plugins::draw::init(ctx, platform, AlloyContext(atx), input_state, engine_state))
        .plugin(move |ctx| plugins::tree::init(&ctx, render_tree, tree_cmd_tx, tree_platform, tree_atx))
        .plugin(move |ctx| plugins::texture::init(ctx, texture_atx))
        .plugin(|ctx| plugins::events::init(&ctx))
        .plugin(|ctx| plugins::raf::init(&ctx))
        .userdata(clock.clone());
      #[cfg(feature = "go")]
      {
        let proxy_files = proxy_files_enabled.load(Ordering::Relaxed);
        let proxy_http = proxy_http_enabled.load(Ordering::Relaxed);
        if proxy_files || proxy_http {
          if let Some(url) = dev_server.get().cloned() {
            if proxy_files {
              builder = builder.module_override("flux:fs", go::ProxyFsModule);
            }
            builder = builder.plugin(move |ctx| go::install_proxy_state(ctx, url, proxy_http));
          }
        }
      }
      let engine = builder.build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();

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

pub fn start(rt: &tokio::runtime::Runtime, app_source: Option<AppSource>, mode: alloy::Mode, size: (u32, u32)) {
  alloy::install_logger();
  let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
  log::info!("[srt] SolidRT version {version}");

  let handle = rt.handle().clone();
  let record_fps = match &mode {
    alloy::Mode::Record(record) => Some(record.fps),
    _ => None,
  };
  let app = alloy::setup("SolidRT", ISize::new(size.0 as i64, size.1 as i64), mode);

  app.run(move |atx, alloy_cmd_tx, event_rx| {
    ui_thread(handle, atx, alloy_cmd_tx, event_rx, app_source, record_fps);
  });
}
