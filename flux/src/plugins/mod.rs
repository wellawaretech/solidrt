pub mod body;
pub mod console;
pub mod fetch;
pub mod flux;
pub mod headers;
pub mod http;
pub mod request;
pub mod response;
pub mod text;
pub mod time;

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::{AsyncContext, AsyncRuntime, Ctx, Object};

use crate::engine::ShutdownHooks;
use crate::logger::Logger;
use crate::pending::PendingOps;

pub(crate) type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;
pub(crate) type UserdataFn = Box<dyn for<'js> FnOnce(&Ctx<'js>) + Send>;
pub(crate) type ModuleOverrideFn = Box<dyn FnOnce(&mut BuiltinResolver, &mut ModuleLoader) + Send>;

pub(crate) async fn init_context(
  setups: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  module_overrides: Vec<ModuleOverrideFn>,
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

  resolver.add_module("flux:fs");
  loader.add_module("flux:fs", flux::fs::FsModule);

  resolver.add_module("flux:http");
  loader.add_module("flux:http", flux::serve::HttpModule);

  resolver.add_module("flux:process");
  loader.add_module("flux:process", flux::process::ProcessModule);

  resolver.add_module("flux:path");
  loader.add_module("flux:path", flux::path::PathModule);

  for f in module_overrides {
    f(&mut resolver, &mut loader);
  }

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
      flux::events::init(&ctx);
      flux_obj.set("version", env!("FLUX_VERSION")).expect("failed to set Flux.version");
      headers::init_headers(&ctx);
      request::init_request(&ctx);
      response::init_response(&ctx);
      text::init_text(&ctx);

      ctx.globals().set("Flux", flux_obj).unwrap();

      for setup in setups {
        setup(ctx.clone());
      }
    })
    .await;

  (runtime, context, pending)
}
