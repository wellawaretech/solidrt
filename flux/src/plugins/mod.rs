// Plugin layers (see flux/CLAUDE.md): `standards` = web-standard JS APIs
// (console, fetch, Headers/Request/Response, timers, WebSocket client);
// `modules` = the `flux:*` capability modules (sqlite, http, p2p, ...) binding
// forge; `gui` = the alloy-backed render/capture bindings. js_error + marshal
// are the shared marshalling toolkit used across all three.
pub mod js_error;
pub mod marshal;

pub mod modules;
pub mod standards;
#[cfg(feature = "gui")]
pub mod gui;

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::{Array, AsyncContext, AsyncRuntime, Ctx, Object};

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

  resolver.add_module("flux:sqlite");
  loader.add_module("flux:sqlite", modules::sqlite::SqliteModule);

  resolver.add_module("flux:fs");
  loader.add_module("flux:fs", modules::fs::FsModule);

  resolver.add_module("flux:http");
  loader.add_module("flux:http", modules::serve::HttpModule);

  resolver.add_module("flux:p2p");
  loader.add_module("flux:p2p", modules::p2p::P2pModule);

  resolver.add_module("flux:process");
  loader.add_module("flux:process", modules::process::ProcessModule);

  resolver.add_module("flux:path");
  loader.add_module("flux:path", modules::path::PathModule);

  resolver.add_module("flux:subprocess");
  loader.add_module("flux:subprocess", modules::subprocess::SubprocessModule);

  for f in module_overrides {
    f(&mut resolver, &mut loader);
  }

  runtime.set_loader(resolver, loader).await;

  let context = AsyncContext::full(&runtime).await.expect("failed to create JS context");

  let pending = PendingOps::new();

  context
    .with(|ctx| {
      ctx.store_userdata(pending.clone()).unwrap();
      crate::logger::store_logger(&ctx, logger);
      ctx.store_userdata(shutdown_hooks).unwrap();
      for store in userdata {
        store(&ctx);
      }
      let flux_obj = Object::new(ctx.clone()).unwrap();

      standards::http::init_http(&ctx);
      standards::time::init(&ctx);
      standards::fetch::init_fetch(&ctx);
      standards::console::init_console(&ctx);
      modules::events::init(&ctx);
      flux_obj.set("version", env!("FLUX_VERSION")).expect("failed to set Flux.version");
      flux_obj.set("capabilities", build_capabilities(&ctx)).expect("failed to set Flux.capabilities");
      standards::headers::init_headers(&ctx);
      standards::request::init_request(&ctx);
      standards::response::init_response(&ctx);
      standards::text::init_text(&ctx);
      standards::websocket::init_websocket(&ctx);

      ctx.globals().set("Flux", flux_obj).unwrap();

      for setup in setups {
        setup(ctx.clone());
      }
    })
    .await;

  (runtime, context, pending)
}

/// Feature names this build/runtime provides, surfaced as `Flux.capabilities`.
/// JS branches on availability (`Flux.capabilities.includes("subprocess")`)
/// rather than on the OS. A conditionally-compiled feature would be added under
/// its own cfg, so it only appears when actually present.
fn build_capabilities<'js>(ctx: &Ctx<'js>) -> Array<'js> {
  let names = ["sqlite", "fs", "http", "p2p", "process", "path", "subprocess"];
  let arr = Array::new(ctx.clone()).expect("create capabilities array");
  for (i, name) in names.iter().enumerate() {
    arr.set(i, *name).expect("set capability");
  }
  arr
}
