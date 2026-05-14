mod go;
mod plugins;
mod rendertree;

use alloy::impellers::{DisplayListBuilder, ISize};
use alloy::log;
use flux::rquickjs::JsLifetime;
use flux::{emit_event, ExecHandle, FluxEngine};
use rendertree::{PlatformContext, RenderTree};
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
  atx: Arc<alloy::Context>,
  source: Option<String>,
) {
  let platform = Arc::new(PlatformContext::new());
  let src = source.as_deref().unwrap_or(DEFAULT_SOURCE);

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    local.spawn_local(async {
      loop {
        while let Some(event) = alloy::sdl_utils::poll_event() {
          match event {
            alloy::sdl3::event::Event::Quit { .. } => std::process::exit(0),
            alloy::sdl3::event::Event::KeyDown { keycode, .. } => log!("[key] {keycode:?}"),
            _ => {}
          }
        }
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
      }
    });

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<go::GoCmd>();
    #[cfg(feature = "go")]
    local.spawn_local({
      let cmd_tx = cmd_tx.clone();
      async move {
        loop {
          tokio::time::sleep(std::time::Duration::from_secs(5)).await;
          let _ = cmd_tx.send(go::GoCmd::Stop);
        }
      }
    });

    loop {
      let mut render_tree = RenderTree::new();
      {
        let mut builder = DisplayListBuilder::new(None);
        rendertree::composite::composite(&mut builder, &mut render_tree, &platform);
        if let Some(dl) = builder.build() {
          atx.submit(dl).expect("Failed to submit display list");
        }
      }

      let engine = FluxEngine::builder()
        .logger(|_level, msg| log!("[js] {msg}"))
        .plugin({
          let platform = platform.clone();
          let atx = atx.clone();
          move |ctx| plugins::draw::init(ctx, platform.clone(), AlloyContext(atx.clone()))
        })
        .plugin(move |ctx| plugins::tree::init(&ctx, render_tree))
        .build();
      exec_tx.send(engine.exec_handle()).ok();

      local.run_until(async {
        tokio::select! {
          _ = engine.eval_source(src) => {}
          Some(_) = cmd_rx.recv() => {}
        }
      }).await;
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
  let handle = rt.handle().clone();
  let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));
  let start_time = std::time::Instant::now();
  let (exec_tx, exec_rx) = std::sync::mpsc::channel::<ExecHandle>();
  let mut current_exec: Option<ExecHandle> = None;

  app.run(
    move |atx| {
      ui_thread(handle, exec_tx, atx, source);
    },
    move |display, dl| {
      main_thread(&mut current_exec, &exec_rx, start_time, display, dl);
    },
  );
}
