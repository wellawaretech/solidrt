use qjsrt::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use qjsrt::{JsEngine, on_shutdown};

#[derive(Clone, JsLifetime)]
struct Identity(#[qjs(skip_trace)] String);

fn whoami_plugin(ctx: Ctx<'_>) {
    ctx.store_userdata(Identity("World".into())).unwrap();

    let whoami_fn = Function::new(
        ctx.clone(),
        MutFn::from(|ctx: Ctx<'_>| -> String {
            ctx.userdata::<Identity>().unwrap().0.clone()
        }),
    )
    .unwrap();

    ctx.globals().set("whoami", whoami_fn).unwrap();

    on_shutdown(&ctx, || println!("shutdown: plugin cleanup complete"));
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let local = tokio::task::LocalSet::new();
    local.run_until(async {
        let mut engine = JsEngine::builder()
            .plugin(whoami_plugin)
            .build();

        engine.eval_source(r#"
            console.log(`Hello, ${whoami()}!`)
        "#);

        engine.run().await;
    }).await;
}
