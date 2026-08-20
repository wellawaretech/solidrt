// Shared plugin infrastructure: the marshalling toolkit (js_error, marshal,
// value, seekable) every plugin layer uses, and `init_context`, which builds the
// JS context and registers the layers. The layers themselves are crate-level
// siblings named for what they marshal (see flux/CLAUDE.md):
// `standards_plugins` = web-standard JS APIs (console, fetch, Headers/Request/
// Response, timers, WebSocket client), whatever backs them; `forge_plugins` =
// the `flux:*` capability modules binding forge; `alloy_plugins` = the
// alloy-backed render/capture bindings (feature `gui`).
pub mod js_error;
pub mod marshal;
pub mod seekable;
pub mod value;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::{Array, AsyncContext, AsyncRuntime, Ctx, JsLifetime, Object, Value};

use crate::engine::ShutdownHooks;
use crate::logger::{Logger, UncaughtHook};
use crate::pending::PendingOps;

pub(crate) type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;
pub(crate) type UserdataFn = Box<dyn for<'js> FnOnce(&Ctx<'js>) + Send>;
pub(crate) type ModuleOverrideFn = Box<dyn FnOnce(&mut BuiltinResolver, &mut ModuleLoader) + Send>;

/// Pending unhandled promise rejections, keyed by promise identity, awaiting the
/// next microtask checkpoint. The value is the already-formatted message. See
/// `set_host_promise_rejection_tracker` below and `engine::flush_rejections`.
#[derive(Clone, JsLifetime)]
pub(crate) struct RejectionLog(#[qjs(skip_trace)] pub Arc<Mutex<HashMap<u64, String>>>);

/// A stable identity for a JS value across tracker calls. `Value`'s `Hash` keys
/// on tag plus pointer bits, so the same promise object hashes the same on its
/// reject and its later handle; distinct objects effectively never collide.
fn value_identity(value: &Value<'_>) -> u64 {
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  value.hash(&mut hasher);
  hasher.finish()
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn init_context(
  setups: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  module_overrides: Vec<ModuleOverrideFn>,
  logger: Logger,
  stack_size: Option<usize>,
  memory_limit: Option<usize>,
  interrupt: Option<Arc<AtomicBool>>,
  on_uncaught: Option<UncaughtHook>,
  shutdown_hooks: ShutdownHooks,
) -> (AsyncRuntime, AsyncContext, PendingOps, RejectionLog) {
  let runtime = AsyncRuntime::new().expect("failed to create JS runtime");

  if let Some(limit) = stack_size {
    runtime.set_max_stack_size(limit).await;
  }

  if let Some(limit) = memory_limit {
    runtime.set_memory_limit(limit).await;
  }

  if let Some(flag) = interrupt {
    runtime.set_interrupt_handler(Some(Box::new(move || flag.load(Ordering::Relaxed)))).await;
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
  let rejections = RejectionLog(Arc::new(Mutex::new(HashMap::new())));
  {
    let rejections = rejections.clone();
    runtime
      .set_host_promise_rejection_tracker(Some(Box::new(
        move |_ctx: Ctx<'_>, promise: Value<'_>, reason: Value<'_>, is_handled: bool| {
          let key = value_identity(&promise);
          let mut pending = rejections.0.lock().expect("rejection log poisoned");
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
  loader.add_module("flux:sqlite", crate::forge_plugins::sqlite::SqliteModule);

  resolver.add_module("flux:fs");
  loader.add_module("flux:fs", crate::forge_plugins::fs::FsModule);

  resolver.add_module("flux:http");
  loader.add_module("flux:http", crate::forge_plugins::serve::HttpModule);

  resolver.add_module("flux:p2p");
  loader.add_module("flux:p2p", crate::forge_plugins::p2p::P2pModule);

  resolver.add_module("flux:net");
  loader.add_module("flux:net", crate::forge_plugins::net::NetModule);

  resolver.add_module("flux:mdns");
  loader.add_module("flux:mdns", crate::forge_plugins::mdns::MdnsModule);

  resolver.add_module("flux:process");
  loader.add_module("flux:process", crate::forge_plugins::process::ProcessModule);

  resolver.add_module("flux:path");
  loader.add_module("flux:path", crate::forge_plugins::path::PathModule);

  resolver.add_module("flux:subprocess");
  loader.add_module("flux:subprocess", crate::forge_plugins::subprocess::SubprocessModule);

  resolver.add_module("flux:svg");
  loader.add_module("flux:svg", crate::forge_plugins::svg::SvgModule);

  resolver.add_module("flux:image");
  loader.add_module("flux:image", crate::forge_plugins::image::ImageModule);

  resolver.add_module("flux:wasm");
  loader.add_module("flux:wasm", crate::forge_plugins::wasm::WasmModuleDef);

  resolver.add_module("flux:ffi");
  loader.add_module("flux:ffi", crate::forge_plugins::ffi::FfiModuleDef);

  resolver.add_module("flux:isolate");
  loader.add_module("flux:isolate", crate::forge_plugins::isolate::IsolateModule);

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
      if let Some(hook) = on_uncaught {
        crate::logger::store_uncaught_hook(&ctx, hook);
      }
      ctx.store_userdata(rejections.clone()).expect("store rejection log");
      ctx.store_userdata(shutdown_hooks).unwrap();
      for store in userdata {
        store(&ctx);
      }
      let flux_obj = Object::new(ctx.clone()).unwrap();

      crate::standards_plugins::http::init_http(&ctx);
      crate::standards_plugins::time::init(&ctx);
      crate::standards_plugins::fetch::init_fetch(&ctx);
      crate::standards_plugins::console::init_console(&ctx);
      crate::forge_plugins::events::init(&ctx);
      flux_obj.set("version", env!("FLUX_VERSION")).expect("failed to set Flux.version");
      flux_obj.set("capabilities", build_capabilities(&ctx)).expect("failed to set Flux.capabilities");
      crate::standards_plugins::headers::init_headers(&ctx);
      crate::standards_plugins::request::init_request(&ctx);
      crate::standards_plugins::response::init_response(&ctx);
      crate::standards_plugins::text::init_text(&ctx);
      crate::standards_plugins::websocket::init_websocket(&ctx);

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
  &["sqlite", "fs", "http", "p2p", "process", "path", "subprocess", "svg", "image", "wasm", "ffi", "isolate"];

fn build_capabilities<'js>(ctx: &Ctx<'js>) -> Array<'js> {
  let arr = Array::new(ctx.clone()).expect("create capabilities array");
  for (i, name) in BASE_CAPABILITIES.iter().enumerate() {
    arr.set(i, *name).expect("set capability");
  }
  arr
}
