use std::sync::Arc;
use qjsrt::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use qjsrt::JsEngine;

// Wrap Rust types with JsLifetime to store them in the JS context via userdata.
// skip_trace tells rquickjs this field holds no JS values that need GC tracing.
#[derive(Clone, JsLifetime)]
struct Identity(#[qjs(skip_trace)] String);

fn main() {
    let rt = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime"),
    );

    let (engine, session) = JsEngine::builder(rt.clone())
        .plugin(move |ctx| {
            // Store Rust state in the JS context — retrievable by type from any JS function
            ctx.store_userdata(Identity("qjsrt".into())).unwrap();

            // Define a JS function that reads back the stored userdata
            let whoami_fn = Function::new(
                ctx.clone(),
                MutFn::from(|ctx: Ctx<'_>| -> String {
                    ctx.userdata::<Identity>().unwrap().0.clone()
                }),
            )
            .unwrap();

            // Expose it as a global function callable from JS
            ctx.globals().set("whoami", whoami_fn).unwrap();
        })
        .build();

    let handle = std::thread::spawn(move || session.run());

    rt.block_on(async {
        engine.eval_source(r#"
            console.log(whoami())
        "#).await;
        drop(engine);
    });

    handle.join().unwrap();
}
