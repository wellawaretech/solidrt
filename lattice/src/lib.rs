mod frame;
mod plugins;
mod rendertree;
#[cfg(feature = "go")]
mod go;

enum EngineCmd {
  Stop,
  Reload(String),
}

use alloy::impellers::{ISize, Rect};
use frame::{EngineState, InputEvent, InputState};
use flux::rquickjs::JsLifetime;
use flux::{emit_event, ExecHandle, FluxEngine};
use rendertree::{PlatformContext, RenderTree};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

// --- Start Android entry point ------------------------------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn SDL_main(_argc: i32, _argv: *mut *mut i8) -> i32 {
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();
  start(&rt, None);
  0
}

// --- End Android entry point ------------------------------

#[derive(Clone, JsLifetime)]
struct AlloyContext(#[qjs(skip_trace)] Arc<alloy::Context>);

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
) {
  let platform = Arc::new(PlatformContext::new());
  let input_state = Arc::new(InputState::new());
  let mut current_src = source.unwrap_or_else(|| DEFAULT_SOURCE.to_string());
  let start_time = std::time::Instant::now();

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
      loop {
        while let Ok(event) = event_rx.try_recv() {
          match event {
            alloy::AlloyEvent::Quit => std::process::exit(0),
            alloy::AlloyEvent::Resize { size, safe_area, display_scale } => {
              platform_events.set_window_size(size.width as f32, size.height as f32);
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
            alloy::AlloyEvent::FrameRendered { frame } => {
              if let Some(eh) = current_exec_events.borrow().as_ref() {
                let elapsed = start_time.elapsed().as_secs_f64();
                eh.exec(move |ctx| {
                  let obj = rquickjs::Object::new(ctx.clone()).expect("create object");
                  obj.set("frame", frame).expect("set frame");
                  obj.set("time", elapsed).expect("set time");
                  emit_event(&ctx, "render", obj);
                });
              }
            }
          }
        }
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
      }
    });

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<EngineCmd>();
    #[cfg(feature = "go")]
    go::start(&handle, cmd_tx.clone());

    loop {
      let render_tree = RenderTree::new();
      let platform = platform.clone();
      let atx = atx.clone();
      let input_state = input_state.clone();
      let engine_state = Arc::new(EngineState::new());
      *current_engine_state.borrow_mut() = Some(engine_state.clone());

      let engine = FluxEngine::builder()
        .logger(|level, msg| match level {
          flux::LogLevel::Debug => log::debug!("{msg}"),
          flux::LogLevel::Log => log::info!("{msg}"),
          flux::LogLevel::Warn => log::warn!("{msg}"),
          flux::LogLevel::Error => log::error!("{msg}"),
        })
        .plugin(move |ctx| plugins::draw::init(ctx, platform, AlloyContext(atx), input_state, engine_state))
        .plugin(move |ctx| plugins::tree::init(&ctx, render_tree))
        .build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();

      let mut next_src: Option<String> = None;
      local
        .run_until(async {
          tokio::select! {
            _ = engine.eval_source(&current_src) => {}
            Some(cmd) = cmd_rx.recv() => {
              if let EngineCmd::Reload(src) = cmd { next_src = Some(src); }
            }
          }
        })
        .await;
      if let Some(src) = next_src { current_src = src; }
    }
  });
}

pub fn start(rt: &tokio::runtime::Runtime, source: Option<String>) {
  alloy::install_logger();
  let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
  log::info!("[srt] SolidRT version {version}");

  let handle = rt.handle().clone();
  let app = alloy::setup("SolidRT", ISize::new(1200, 800));

  app.run(
    move |atx, alloy_cmd_tx, event_rx| {
      ui_thread(handle, atx, alloy_cmd_tx, event_rx, source);
    },
    alloy::RenderHooks {
      pre_render: Box::new(|_, _| {}),
      post_render: Box::new(|_, _| {}),
    },
  );
}
