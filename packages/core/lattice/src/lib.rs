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

pub fn start() {
    let rt = alloy::setup("Alloy + Flux demo", ISize::new(1200, 800));

    rt.run(
        |atx| {
            let engine = JsEngine::builder()
                .logger(|_level, msg| log!("[js] {msg}"))
                .userdata(AlloyContext(atx))
                .plugin(plugin)
                .build();

            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(engine.eval_source("draw()"));
        },
        |display, dl| {
            display
                .draw_display_list(dl)
                .expect("Failed to draw display list");
            display.present();
        },
    );
}
