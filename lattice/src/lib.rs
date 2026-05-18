mod plugins;
mod rendertree;
#[cfg(feature = "go")]
mod go;

enum EngineCmd {
  Stop,
  Reload(String),
}

use alloy::impellers::ISize;
use alloy::log;
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

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  event_rx: std::sync::mpsc::Receiver<alloy::Event>,
  source: Option<String>,
) {
  let platform = Arc::new(PlatformContext::new());
  let mut current_src = source.unwrap_or_else(|| DEFAULT_SOURCE.to_string());
  let start_time = std::time::Instant::now();

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    let current_exec: Rc<RefCell<Option<ExecHandle>>> = Rc::new(RefCell::new(None));
    let current_exec_events = current_exec.clone();

    let platform_events = platform.clone();
    local.spawn_local(async move {
      loop {
        while let Ok(event) = event_rx.try_recv() {
          match event {
            alloy::Event::Quit => std::process::exit(0),
            alloy::Event::Resize { size, safe_area, display_scale } => {
              platform_events.set_window_size(size.width as f32, size.height as f32);
              if let Some(eh) = current_exec_events.borrow().as_ref() {
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
            }
            alloy::Event::KeyDown { keycode, .. } => {
              if let Some(eh) = current_exec_events.borrow().as_ref() {
                let key = format!("{keycode:?}");
                eh.exec(move |ctx| emit_event(&ctx, "keydown", key));
              }
            }
            alloy::Event::FrameRendered { frame } => {
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
            _ => {}
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

      let engine = FluxEngine::builder()
        .logger(|_level, msg| log!("{msg}"))
        .plugin(move |ctx| plugins::draw::init(ctx, platform, AlloyContext(atx)))
        .plugin(move |ctx| plugins::tree::init(&ctx, render_tree))
        .build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());

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
  let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
  log!("[srt] SolidRT version {version}");

  let handle = rt.handle().clone();
  let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));

  app.run(
    move |atx, event_rx| {
      ui_thread(handle, atx, event_rx, source);
    },
    alloy::RenderHooks {
      pre_render: Box::new(|| {}),
      post_render: Box::new(|| {}),
    },
  );
}
