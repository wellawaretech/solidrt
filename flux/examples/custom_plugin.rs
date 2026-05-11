use flux::rquickjs::{function::MutFn, Ctx, Function, JsLifetime};
use flux::{on_shutdown, CtxLogger, FluxEngine};

#[derive(Clone, JsLifetime)]
struct Identity(#[qjs(skip_trace)] String);

fn whoami_plugin(ctx: Ctx<'_>) {
  ctx.logger().log("initializing whoami plugin");

  ctx.store_userdata(Identity("World".into())).unwrap();

  let whoami_fn = Function::new(
    ctx.clone(),
    MutFn::from(|ctx: Ctx<'_>| -> String {
      ctx.logger().log("calling whoami");
      ctx.userdata::<Identity>().unwrap().0.clone()
    }),
  )
  .unwrap();

  ctx.globals().set("whoami", whoami_fn).unwrap();

  on_shutdown(&ctx, |logger| {
    logger.log("shutdown: plugin cleanup complete")
  });
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let engine = FluxEngine::builder().plugin(whoami_plugin).build();

  engine
    .eval_source(r#"console.log(`Hello, ${whoami()}!`)"#)
    .await;
}
