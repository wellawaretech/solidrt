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
  exec_tx: std::sync::mpsc::Sender<ExecHandle>,
  event_rx: tokio::sync::mpsc::UnboundedReceiver<alloy::AppEvent>,
  atx: Arc<alloy::Context>,
  source: Option<String>,
) {
  let platform = Arc::new(PlatformContext::new());
  let mut current_src = source.unwrap_or_else(|| DEFAULT_SOURCE.to_string());

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    let eh_slot: Rc<RefCell<Option<ExecHandle>>> = Rc::new(RefCell::new(None));
    let eh_slot_events = eh_slot.clone();

    local.spawn_local(async move {
      let mut event_rx = event_rx;
      while let Some(event) = event_rx.recv().await {
        let Some(eh) = eh_slot_events.borrow().as_ref().map(ExecHandle::clone) else { continue };
        match event {
          alloy::AppEvent::Resize { width, height, safe_area } => {
            let payload = format!(
              r#"{{"width":{width},"height":{height},"safeArea":{{"top":{t},"right":{r},"bottom":{b},"left":{l}}}}}"#,
              t = safe_area.top, r = safe_area.right, b = safe_area.bottom, l = safe_area.left,
            );
            eh.exec(move |ctx| emit_event(&ctx, "resize", payload));
          }
        }
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
        .logger(|_level, msg| log!("[js] {msg}"))
        .plugin(move |ctx| plugins::draw::init(ctx, platform, AlloyContext(atx)))
        .plugin(move |ctx| plugins::tree::init(&ctx, render_tree))
        .build();
      let eh = engine.exec_handle();
      *eh_slot.borrow_mut() = Some(eh.clone());
      exec_tx.send(eh).ok();

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

fn main_thread(
  current_exec: &mut Option<ExecHandle>,
  exec_rx: &std::sync::mpsc::Receiver<ExecHandle>,
  start_time: std::time::Instant,
  display: &mut dyn alloy::RenderSurface,
  dl: &alloy::impellers::DisplayList,
) {
  if let Ok(new_exec) = exec_rx.try_recv() {
    *current_exec = Some(new_exec);
  }

  display
    .draw_display_list(dl)
    .expect("Failed to draw display list");

  display.present();

  if let Some(eh) = current_exec {
    let t = start_time.elapsed().as_secs_f64().to_string();
    eh.exec(move |ctx| emit_event(&ctx, "render", t));
  }
}

pub fn start(rt: &tokio::runtime::Runtime, source: Option<String>) {
  let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
  log!("[SolidRT] version {version}");

  let handle = rt.handle().clone();
  let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));
  let start_time = std::time::Instant::now();
  let (exec_tx, exec_rx) = std::sync::mpsc::channel::<ExecHandle>();
  let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<alloy::AppEvent>();
  let mut current_exec: Option<ExecHandle> = None;

  app.run(
    move |atx| {
      ui_thread(handle, exec_tx, event_rx, atx, source);
    },
    move |event| { event_tx.send(event).ok(); },
    move |display, dl| {
      main_thread(&mut current_exec, &exec_rx, start_time, display, dl);
    },
  );
}
