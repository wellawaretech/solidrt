pub mod body;
pub mod console;
pub mod fetch;
pub mod flux;
pub mod headers;
pub mod http;
pub mod request;
pub mod response;
pub mod time;

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::{AsyncContext, AsyncRuntime, Ctx, Object};

use crate::engine::ShutdownHooks;
use crate::logger::Logger;
use crate::pending::PendingOps;

pub(crate) type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;
pub(crate) type UserdataFn = Box<dyn for<'js> FnOnce(&Ctx<'js>) + Send>;

pub(crate) async fn init_context(
  setups: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  logger: Logger,
  stack_size: Option<usize>,
  shutdown_hooks: ShutdownHooks,
) -> (AsyncRuntime, AsyncContext, PendingOps) {
  let runtime = AsyncRuntime::new().expect("failed to create JS runtime");

  if let Some(limit) = stack_size {
    runtime.set_max_stack_size(limit).await;
  }

  let mut resolver = BuiltinResolver::default();
  let mut loader = ModuleLoader::default();

  resolver.add_module("flux:memory");
  loader.add_module("flux:memory", flux::memory::MemoryModule);

  resolver.add_module("flux:sqlite");
  loader.add_module("flux:sqlite", flux::sqlite::SqliteModule);

  runtime.set_loader(resolver, loader).await;

  let context = AsyncContext::full(&runtime).await.expect("failed to create JS context");

  let pending = PendingOps::new();

  context
    .with(|ctx| {
      ctx.store_userdata(pending.clone()).unwrap();
      ctx.store_userdata(logger).unwrap();
      ctx.store_userdata(shutdown_hooks).unwrap();
      for store in userdata {
        store(&ctx);
      }
      let flux_obj = Object::new(ctx.clone()).unwrap();

      http::init_http(&ctx);
      time::init(&ctx);
      fetch::init_fetch(&ctx);
      console::init_console(&ctx);
      flux::events::init_events(&ctx, &flux_obj);
      flux::file::init_file(&ctx, &flux_obj);
      flux::dir::init_dir(&ctx, &flux_obj);
      flux::write::init_write(&ctx, &flux_obj);
      headers::init_headers(&ctx);
      request::init_request(&ctx);
      response::init_response(&ctx);
      flux::serve::init_serve(&ctx, &flux_obj);

      ctx.globals().set("Flux", flux_obj).unwrap();

      for setup in setups {
        setup(ctx.clone());
      }
    })
    .await;

  (runtime, context, pending)
}
