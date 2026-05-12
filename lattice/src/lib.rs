mod plugins;
mod rendertree;

use alloy::impellers::{Color, DisplayListBuilder, ISize, Paint, Point, Rect, Size};
use alloy::log;
use flux::rquickjs::{Ctx as QuickJsContext, Function, JsLifetime};
use flux::{emit_event, ExecHandle, FluxEngine};
use rendertree::{PlatformContext, RenderTree};
use std::sync::{Arc, OnceLock};

#[derive(Clone, JsLifetime)]
struct AlloyContext(#[qjs(skip_trace)] Arc<alloy::Context>);

impl std::ops::Deref for AlloyContext {
  type Target = alloy::Context;
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

pub fn plugin(qtx: QuickJsContext<'_>) {
  let draw_fn = Function::new(qtx.clone(), |qtx: QuickJsContext<'_>| {
    let tree = qtx.userdata::<plugins::tree::SharedRenderTree>().unwrap();
    let atx = qtx.userdata::<AlloyContext>().unwrap();
    let platform = PlatformContext::new();
    let mut builder = DisplayListBuilder::new(None);
    rendertree::composite::composite(&mut builder, &mut tree.0.borrow_mut(), &platform);
    if let Some(dl) = builder.build() {
      atx.submit(dl).expect("Failed to submit display list");
    }
  })
  .unwrap();

  let globals = qtx.globals();
  globals.set("draw", draw_fn).unwrap();
}

// const SOURCE: &str = "setInterval(draw, 100)";
// const SOURCE: &str = "Flux.on('render', draw); draw()";

const SOURCE: &str = "
createRoot(1);
createNode(2, 'rect');
setProperty(2, 'x', 200);
setProperty(2, 'y', 200);
setProperty(2, 'w', 200);
setProperty(2, 'h', 200);
setProperty(2, 'color', 0x00ff00ff);
insertNode(1, 2);
Flux.on('render', draw);
draw();
";

pub fn start(rt: &tokio::runtime::Runtime) {
  let handle = rt.handle().clone();
  let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));
  let start_time = std::time::Instant::now();
  let exec_handle: Arc<OnceLock<ExecHandle>> = Arc::new(OnceLock::new());
  let exec_handle_for_setup = exec_handle.clone();

  let mut render_tree = RenderTree::new();
  // let window_id = render_tree.add_node(1, rendertree::Window::default().with_layout());
  // render_tree.root = Some(window_id);
  // let mut rect = rendertree::Rectangle::default();
  // rect.paint.color = alloy::impellers::Color::new_srgba(0.0, 0.8, 0.0, 1.0);
  // let mut rect_elem = rect.with_layout();
  // rect_elem.layout_data_mut().style.flex_grow = 1.0;
  // let rect_id = render_tree.add_node(2, rect_elem);
  // render_tree.insert_node(window_id, rect_id, None);
  // let mut rect2 = rendertree::Rectangle::default();
  // rect2.paint.color = alloy::impellers::Color::new_srgba(0.0, 0.0, 0.8, 1.0);
  // let mut rect2_elem = rect2.with_layout();
  // rect2_elem.layout_data_mut().style.flex_grow = 1.0;
  // let rect2_id = render_tree.add_node(3, rect2_elem);
  // render_tree.insert_node(window_id, rect2_id, None);

  app.run(
    move |atx| {
      let platform = PlatformContext::new();
      {
        let mut builder = DisplayListBuilder::new(None);
        rendertree::composite::composite(&mut builder, &mut render_tree, &platform);
        if let Some(dl) = builder.build() {
          atx.submit(dl).expect("Failed to submit display list");
        }
      }

      let engine = FluxEngine::builder()
        .logger(|_level, msg| log!("[js] {msg}"))
        .userdata(AlloyContext(atx))
        .plugin(plugin)
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
        local.run_until(engine.eval_source(SOURCE)).await;
      });
    },
    move |display, dl| {
      display
        .draw_display_list(dl)
        .expect("Failed to draw display list");
      display.present();
      if let Some(eh) = exec_handle.get() {
        let t = start_time.elapsed().as_secs_f64().to_string();
        eh.exec(move |ctx| emit_event(&ctx, "render", t));
      }
    },
  );
}
