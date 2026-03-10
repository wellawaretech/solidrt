use qjsrt::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use qjsrt::JsEngine;

// Wrap Rust types with JsLifetime to store them in the JS context via userdata.
// skip_trace tells rquickjs this field holds no JS values that need GC tracing.
#[derive(Clone, JsLifetime)]
struct Identity(#[qjs(skip_trace)] String);

fn main() {
    let engine = JsEngine::builder()
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

    // eval_detached sends code to the engine thread and returns immediately
    // with a oneshot receiver that signals when evaluation is complete
    let mut done_rx = engine.eval_detached(r#"
        print(whoami())
    "#);

    // Poll for completion — the engine runs on its own thread
    loop {
        match done_rx.try_recv() {
            Ok(_) => break,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }
}
