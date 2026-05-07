use std::sync::Arc;

use alloy::impellers::{Color, DisplayListBuilder, ISize, Paint, Point, Rect, Size};
use alloy::log;
use flux::rquickjs::{Ctx as QuickJsContext, Function, JsLifetime};
use flux::JsEngine;

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
        let mut builder = DisplayListBuilder::new(None);
        let rect = Rect::new(Point::new(200.0, 100.0), Size::new(200.0, 200.0));
        let mut paint = Paint::default();
        paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
        builder.draw_rect(&rect, &paint);
        let dl = builder.build().unwrap();

        let atx = qtx.userdata::<AlloyContext>().unwrap();
        atx.submit(dl).expect("Failed to submit display list");
    })
    .unwrap();

    let globals = qtx.globals();
    globals.set("draw", draw_fn).unwrap();
}

pub fn start(rt: &tokio::runtime::Runtime) {
    let handle = rt.handle().clone();
    let app = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));

    app.run(
        move |atx| {
            let engine = JsEngine::builder()
                .logger(|_level, msg| log!("[js] {msg}"))
                .userdata(AlloyContext(atx))
                .plugin(plugin)
                .build();

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
                    local.run_until(engine.eval_source("setInterval(draw, 100)")).await;
                });
        },
        |display, dl| {
            display
                .draw_display_list(dl)
                .expect("Failed to draw display list");
            display.present();
        },
    );
}
