// Plugin layers (see flux/CLAUDE.md): `standards` = web-standard JS APIs
// (console, fetch, Headers/Request/Response, timers, WebSocket client);
// `modules` = the `flux:*` capability modules (sqlite, http, p2p, ...) binding
// forge; `gui` = the alloy-backed render/capture bindings. js_error + marshal
// are the shared marshalling toolkit used across all three.
pub mod js_error;
pub mod marshal;
pub mod seekable;
pub mod value;

#[cfg(feature = "gui")]
pub mod gui;
pub mod modules;
pub mod standards;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::{Array, AsyncContext, AsyncRuntime, Ctx, Object, Value};

use crate::engine::ShutdownHooks;
use crate::logger::Logger;
use crate::pending::PendingOps;

pub(crate) type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;
pub(crate) type UserdataFn = Box<dyn for<'js> FnOnce(&Ctx<'js>) + Send>;
pub(crate) type ModuleOverrideFn = Box<dyn FnOnce(&mut BuiltinResolver, &mut ModuleLoader) + Send>;

/// Pending unhandled promise rejections, keyed by promise identity, awaiting the
/// next microtask checkpoint. The value is the already-formatted message. See
/// `set_host_promise_rejection_tracker` below and `engine::flush_rejections`.
pub(crate) type RejectionLog = Arc<Mutex<HashMap<u64, String>>>;

/// A stable identity for a JS value across tracker calls. `Value`'s `Hash` keys
/// on tag plus pointer bits, so the same promise object hashes the same on its
/// reject and its later handle; distinct objects effectively never collide.
fn value_identity(value: &Value<'_>) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  value.hash(&mut hasher);
  hasher.finish()
}

pub(crate) async fn init_context(
  setups: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  module_overrides: Vec<ModuleOverrideFn>,
  logger: Logger,
  stack_size: Option<usize>,
  shutdown_hooks: ShutdownHooks,
) -> (AsyncRuntime, AsyncContext, PendingOps, RejectionLog) {
  let runtime = AsyncRuntime::new().expect("failed to create JS runtime");

  if let Some(limit) = stack_size {
    runtime.set_max_stack_size(limit).await;
  }

  // Global safety net for promises that reject with no handler attached. Without
  // this QuickJS swallows them silently (unlike a browser, which logs, or Node,
  // which crashes); only the entry-module promise is otherwise reported.
  //
  // The tracker cannot report on the spot: QuickJS fires is_handled=false the
  // instant a promise rejects unhandled, then is_handled=true if a handler is
  // attached afterwards (e.g. `Promise.reject(e).catch(f)` fires both). Logging
  // immediately would cry wolf on every later-handled rejection. So we record
  // pending rejections here and let the run loop report whatever is still
  // unhandled once the job queue drains (engine::flush_rejections) -- the
  // microtask-checkpoint semantics the HTML spec uses.
  let rejections: RejectionLog = Arc::new(Mutex::new(HashMap::new()));
  {
    let rejections = rejections.clone();
    runtime
      .set_host_promise_rejection_tracker(Some(Box::new(
        move |_ctx: Ctx<'_>, promise: Value<'_>, reason: Value<'_>, is_handled: bool| {
          let key = value_identity(&promise);
          let mut pending = rejections.lock().expect("rejection log poisoned");
          if is_handled {
            pending.remove(&key);
          } else {
            let message = match reason.as_exception() {
              Some(exc) => format!("Uncaught (in promise) {exc}"),
              None => format!("Uncaught (in promise) {reason:?}"),
            };
            pending.insert(key, message);
          }
        },
      )))
      .await;
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

  resolver.add_module("flux:net");
  loader.add_module("flux:net", modules::net::NetModule);

  resolver.add_module("flux:mdns");
  loader.add_module("flux:mdns", modules::mdns::MdnsModule);

  resolver.add_module("flux:process");
  loader.add_module("flux:process", modules::process::ProcessModule);

  resolver.add_module("flux:path");
  loader.add_module("flux:path", modules::path::PathModule);

  resolver.add_module("flux:subprocess");
  loader.add_module("flux:subprocess", modules::subprocess::SubprocessModule);

  resolver.add_module("flux:svg");
  loader.add_module("flux:svg", modules::svg::SvgModule);

  resolver.add_module("flux:image");
  loader.add_module("flux:image", modules::image::ImageModule);

  resolver.add_module("flux:wasm");
  loader.add_module("flux:wasm", modules::wasm::WasmModuleDef);

  resolver.add_module("flux:ffi");
  loader.add_module("flux:ffi", modules::ffi::FfiModuleDef);

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

  (runtime, context, pending, rejections)
}

/// Feature names every flux build provides, surfaced as `Flux.capabilities`.
/// JS branches on availability (`Flux.capabilities.includes("subprocess")`)
/// rather than on the OS. A conditionally-compiled feature would be added under
/// its own cfg, so it only appears when actually present.
pub const BASE_CAPABILITIES: &[&str] =
  &["sqlite", "fs", "http", "p2p", "process", "path", "subprocess", "svg", "image", "wasm", "ffi"];

fn build_capabilities<'js>(ctx: &Ctx<'js>) -> Array<'js> {
  let arr = Array::new(ctx.clone()).expect("create capabilities array");
  for (i, name) in BASE_CAPABILITIES.iter().enumerate() {
    arr.set(i, *name).expect("set capability");
  }
  arr
}
