mod plugins;
mod rendertree;

use alloy::impellers::{Color, DisplayListBuilder, ISize, Paint, Point, Rect, Size};
use alloy::log;
use flux::rquickjs::{Ctx as QuickJsContext, JsLifetime};
use flux::{emit_event, ExecHandle, FluxEngine};
use rendertree::{PlatformContext, RenderTree};
use std::sync::{Arc, OnceLock};

// --- Start Android entry point ------------------------------

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn SDL_main(_argc: i32, _argv: *mut *mut i8) -> i32 {
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();
  start(&rt);
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

#[cfg(feature = "go")]
const DEFAULT_SOURCE: &str = include_str!("../default-app/app.srt.js");

fn ui_thread(
  handle: tokio::runtime::Handle,
  exec_handle_for_setup: Arc<OnceLock<ExecHandle>>,
  atx: Arc<alloy::Context>,
) {
  let mut render_tree = RenderTree::new();
  let platform = Arc::new(PlatformContext::new());
  {
    let mut builder = DisplayListBuilder::new(None);
    rendertree::composite::composite(&mut builder, &mut render_tree, &platform);
    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  }

  let engine = FluxEngine::builder()
    .logger(|_level, msg| log!("[js] {msg}"))
    .plugin(move |ctx| plugins::draw::init(ctx, platform.clone(), AlloyContext(atx)))
    .plugin(move |ctx| plugins::tree::init(&ctx, render_tree))
    .build();

  exec_handle_for_setup.set(engine.exec_handle()).ok();

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    local.spawn_local(async {
      loop {
        while let Some(event) = alloy::sdl_utils::poll_event() {
          match event {
            alloy::sdl3::event::Event::Quit { .. } => {
              std::process::exit(0);
            }
            alloy::sdl3::event::Event::KeyDown { keycode, .. } => {
              log!("[key] {keycode:?}");
            }
            _ => {}
          }
        }
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
      }
    });
    #[cfg(feature = "go")]
    local.run_until(engine.eval_source(DEFAULT_SOURCE)).await;
  });
}

fn main_thread(
  exec_handle: &Arc<OnceLock<ExecHandle>>,
  start_time: std::time::Instant,
  display: &mut dyn alloy::RenderSurface,
  dl: &alloy::impellers::DisplayList,
) {
  display
    .draw_display_list(dl)
    .expect("Failed to draw display list");
  display.present();
  if let Some(eh) = exec_handle.get() {
    let t = start_time.elapsed().as_secs_f64().to_string();
    eh.exec(move |ctx| emit_event(&ctx, "render", t));
  }
}

pub fn start(rt: &tokio::runtime::Runtime) {
  let handle = rt.handle().clone();
  let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));
  let start_time = std::time::Instant::now();
  let exec_handle_main: Arc<OnceLock<ExecHandle>> = Arc::new(OnceLock::new());
  let exec_handle_ui = exec_handle_main.clone();

  app.run(
    move |atx| {
      ui_thread(handle, exec_handle_ui, atx);
    },
    move |display, dl| {
      main_thread(&exec_handle_main, start_time, display, dl);
    },
  );
}
