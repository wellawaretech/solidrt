mod rendertree;

use alloy::impellers::{Color, DisplayListBuilder, ISize, Paint, Point, Rect, Size};
use alloy::log;
use flux::rquickjs::{Ctx as QuickJsContext, Function, JsLifetime};
use flux::{emit_event, ExecHandle, FluxEngine};
use rendertree::RenderTree;
use std::cell::RefCell;
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
        // let mut builder = DisplayListBuilder::new(None);
        // let rect = Rect::new(Point::new(200.0, 100.0), Size::new(200.0, 200.0));
        // let mut paint = Paint::default();
        // paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
        // builder.draw_rect(&rect, &paint);
        // let dl = builder.build().unwrap();

        // let atx = qtx.userdata::<AlloyContext>().unwrap();
        // atx.submit(dl).expect("Failed to submit display list");
    })
    .unwrap();

    let globals = qtx.globals();
    globals.set("draw", draw_fn).unwrap();
}

// const SOURCE: &str = "setInterval(draw, 100)";
const SOURCE: &str = "Flux.on('render', draw); draw()";

pub fn start(rt: &tokio::runtime::Runtime) {
    let handle = rt.handle().clone();
    let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));
    let start_time = std::time::Instant::now();
    let exec_handle: Arc<OnceLock<ExecHandle>> = Arc::new(OnceLock::new());
    let exec_handle_for_setup = exec_handle.clone();

    app.run(
        move |atx| {
            let render_tree = RefCell::new(RenderTree::new());
            {
                let mut tree = render_tree.borrow_mut();
                let window_id = tree.add_node(1, rendertree::Window::default().with_layout());
                tree.root = Some(window_id);

                let mut rect = rendertree::Rectangle::default();
                rect.paint.color = alloy::impellers::Color::new_srgba(0.0, 0.8, 0.0, 1.0);
                let mut rect_elem = rect.with_layout();
                rect_elem.layout_data_mut().style.flex_grow = 1.0;
                let rect_id = tree.add_node(2, rect_elem);
                tree.insert_node(window_id, rect_id, None);

                let mut rect2 = rendertree::Rectangle::default();
                rect2.paint.color = alloy::impellers::Color::new_srgba(0.0, 0.0, 0.8, 1.0);
                let mut rect2_elem = rect2.with_layout();
                rect2_elem.layout_data_mut().style.flex_grow = 1.0;
                let rect2_id = tree.add_node(3, rect2_elem);
                
                tree.insert_node(window_id, rect2_id, None);
            }

            {
                let mut builder = DisplayListBuilder::new(None);
                let mut tree = render_tree.borrow_mut();
                let root_id = tree.root.unwrap();
                rendertree::composite::composite(&mut builder, &mut tree, root_id);
                if let Some(dl) = builder.build() {
                    atx.submit(dl).expect("Failed to submit display list");
                }
            }

            let engine = FluxEngine::builder()
                .logger(|_level, msg| log!("[js] {msg}"))
                .userdata(AlloyContext(atx))
                .plugin(plugin)
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
