mod frame;
#[cfg(feature = "go")]
mod go;
mod overlay;
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// --- Start Android entry point ------------------------------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn SDL_main(_argc: i32, _argv: *mut *mut i8) -> i32 {
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
  start(&rt, None, None, (1280, 720));
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
    emit_event(&ctx, "resize", obj);
  });
}

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  event_rx: std::sync::mpsc::Receiver<alloy::AlloyEvent>,
  source: Option<String>,
  record_fps: Option<u32>,
) {
  #[cfg(feature = "go")]
  let proxy_files_enabled = Arc::new(AtomicBool::new(false));
  #[cfg(feature = "go")]
  let proxy_http_enabled = Arc::new(AtomicBool::new(false));
  let platform = Arc::new(PlatformContext::new());
  let input_state = Arc::new(InputState::new());
  let mut current_src = source.unwrap_or_else(|| DEFAULT_SOURCE.to_string());

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
          alloy::AlloyEvent::FrameRendered { frame, fps, time } => {
            platform_events.set_fps(fps);
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              // FrameRendered reports the frame native just finished
              // drawing. JS uses the "render" event to compute the NEXT
              // frame's state, so shift both fields by +1. The JS-side
              // bootstrap owns frame 0; without the shift, record mode
              // re-runs frame 0 at tick 0 and duplicates a PNG.
              let next_frame = frame + 1;
              // Record mode recomputes a deterministic virtual time so PNGs
              // stay reproducible; live mode forwards the render-thread stamp.
              let time = match record_fps {
                Some(rfps) if rfps > 0 => next_frame as f64 / rfps as f64,
                _ => time,
              };
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("frame", next_frame).expect("set frame");
                obj.set("time", time).expect("set time");
                emit_event(&ctx, "render", obj);
              });
            }
          }
          alloy::AlloyEvent::DisplayRefreshRate { hz } => {
            if let Some(eh) = current_exec_events.borrow().as_ref() {
              eh.exec(move |ctx| {
                let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                obj.set("hz", hz).expect("set hz");
                emit_event(&ctx, "displayRefreshRate", obj);
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
        .logger(|level, msg| match level {
          flux::LogLevel::Debug => log::debug!("{msg}"),
          flux::LogLevel::Log => log::info!("{msg}"),
          flux::LogLevel::Warn => log::warn!("{msg}"),
          flux::LogLevel::Error => log::error!("{msg}"),
        })
        .plugin(move |ctx| plugins::draw::init(ctx, platform, AlloyContext(atx), input_state, engine_state))
        .plugin(move |ctx| plugins::tree::init(&ctx, render_tree, tree_cmd_tx, tree_platform, tree_atx))
        .plugin(move |ctx| plugins::texture::init(ctx, texture_atx));
      #[cfg(feature = "go")]
      {
        let proxy_files = proxy_files_enabled.load(Ordering::Relaxed);
        let proxy_http = proxy_http_enabled.load(Ordering::Relaxed);
        if proxy_files || proxy_http {
          if let Some(url) = dev_server.get().cloned() {
            builder = builder.plugin(move |ctx| go::install_proxy(ctx, url, proxy_files, proxy_http));
          }
        }
      }
      let engine = builder.build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();

      log::info!("[srt] flux engine start");
      let mut next_src: Option<String> = None;
      local
        .run_until(async {
          tokio::select! {
            _ = engine.eval_source(&current_src) => {}
            Some(cmd) = cmd_rx.recv() => {
              match cmd {
                EngineCmd::Reload(src) => { next_src = Some(src); }
                EngineCmd::Stop => { next_src = Some(DEFAULT_SOURCE.to_string()); }
              }
            }
          }
        })
        .await;
      if let Some(src) = next_src {
        current_src = src;
      }
    }
  });
}

pub fn start(
  rt: &tokio::runtime::Runtime,
  source: Option<String>,
  record: Option<alloy::RecordConfig>,
  size: (u32, u32),
) {
  alloy::install_logger();
  let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
  log::info!("[srt] SolidRT version {version}");

  let handle = rt.handle().clone();
  let record_fps = record.as_ref().map(|r| r.fps);
  let mut app = alloy::setup("SolidRT", ISize::new(size.0 as i64, size.1 as i64));
  if let Some(record) = record {
    app = app.with_recording(record);
  }

  app.run(move |atx, alloy_cmd_tx, event_rx| {
    ui_thread(handle, atx, alloy_cmd_tx, event_rx, source, record_fps);
  });
}
