use qjsrt::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use qjsrt::{JsEngine, on_shutdown};

// Wrap Rust types with JsLifetime to store them in the JS context via userdata.
// skip_trace tells rquickjs this field holds no JS values that need GC tracing.
#[derive(Clone, JsLifetime)]
struct Identity(#[qjs(skip_trace)] String);

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local.run_until(async {
        let (engine, session) = JsEngine::builder()
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

                on_shutdown(&ctx, || println!("shutdown: plugin cleanup complete"));
            })
            .build();

        let handle = tokio::task::spawn_local(session.run());

        engine.eval_source(r#"
            console.log(whoami())
        "#).await;

        drop(engine);
        let _ = handle.await;
    }).await;
}
