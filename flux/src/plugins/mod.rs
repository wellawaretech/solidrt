// Shared plugin infrastructure: the marshalling toolkit (js_error, marshal,
// value, seekable) every plugin layer uses, the event bus mechanism (events:
// listener registry and sticky cache, no JS surface of its own), and
// `init_context`, which builds the JS context and registers the layers. The layers themselves are crate-level
// siblings named for what they marshal (see flux/CLAUDE.md):
// `standards_plugins` = web-standard JS APIs (console, fetch, Headers/Request/
// Response, timers, WebSocket client), whatever backs them; `forge_plugins` =
// the `flux:*` capability modules binding forge; `alloy_plugins` = the
// alloy-backed render/capture bindings (feature `gui`); `ore_plugins` = the
// `flux:*` modules binding ore, the OS layer; `test_plugins` =
// flux:test, over flux's own facilities (feature `test`).
pub mod events;
pub mod js_error;
pub mod marshal;
pub mod seekable;
pub mod value;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::object::Property;
use rquickjs::{Array, AsyncContext, AsyncRuntime, Ctx, Function, JsLifetime, Object, Value};

use crate::engine::ShutdownHooks;
use crate::logger::{Logger, UncaughtHook};
use crate::pending::PendingOps;

pub(crate) type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;
pub(crate) type UserdataFn = Box<dyn for<'js> FnOnce(&Ctx<'js>) + Send>;
pub(crate) type ModuleFn = Box<dyn FnOnce(&mut BuiltinResolver, &mut ModuleLoader) + Send>;

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
  modules: Vec<ModuleFn>,
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

  #[cfg(feature = "http")]
  {
    resolver.add_module("flux:http");
    loader.add_module("flux:http", crate::forge_plugins::serve::HttpModule);
  }

  #[cfg(any(feature = "p2p", feature = "p2p-relay"))]
  {
    resolver.add_module("flux:p2p");
    loader.add_module("flux:p2p", crate::forge_plugins::p2p::P2pModule);
  }

  resolver.add_module("flux:net");
  loader.add_module("flux:net", crate::forge_plugins::net::NetModule);

  resolver.add_module("flux:mdns");
  loader.add_module("flux:mdns", crate::forge_plugins::mdns::MdnsModule);

  resolver.add_module("flux:process");
  loader.add_module("flux:process", crate::forge_plugins::process::ProcessModule);

  resolver.add_module("flux:tty");
  loader.add_module("flux:tty", crate::forge_plugins::tty::TtyModule);

  resolver.add_module("flux:path");
  loader.add_module("flux:path", crate::forge_plugins::path::PathModule);

  resolver.add_module("flux:subprocess");
  loader.add_module("flux:subprocess", crate::forge_plugins::subprocess::SubprocessModule);

  resolver.add_module("flux:svg");
  loader.add_module("flux:svg", crate::forge_plugins::svg::SvgModule);

  resolver.add_module("flux:image");
  loader.add_module("flux:image", crate::forge_plugins::image::ImageModule);

  #[cfg(feature = "wasm")]
  {
    resolver.add_module("flux:wasm");
    loader.add_module("flux:wasm", crate::forge_plugins::wasm::WasmModuleDef);
  }

  resolver.add_module("flux:isolate");
  loader.add_module("flux:isolate", crate::forge_plugins::isolate::IsolateModule);

  resolver.add_module("flux:link");
  loader.add_module("flux:link", crate::ore_plugins::link::LinkModule);

  resolver.add_module("flux:haptics");
  loader.add_module("flux:haptics", crate::ore_plugins::haptics::HapticsModule);

  #[cfg(feature = "test")]
  {
    resolver.add_module(crate::test_plugins::MODULE_NAME);
    loader.add_module(crate::test_plugins::MODULE_NAME, crate::test_plugins::TestModule);
  }
  #[cfg(all(feature = "test", feature = "gui"))]
  {
    resolver.add_module(crate::test_plugins::gui::MODULE_NAME);
    loader.add_module(crate::test_plugins::gui::MODULE_NAME, crate::test_plugins::gui::GuiTestModule);
  }

  for f in modules {
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
      ctx.store_userdata(CapabilityRegistry::seeded()).expect("store capability registry");
      crate::ore_plugins::link::store_state(&ctx);
      crate::ore_plugins::haptics::store_state(&ctx);
      crate::standards_plugins::share::store_state(&ctx);
      crate::ore_plugins::availability::store_state(&ctx);
      crate::ore_plugins::permission::store_state(&ctx);
      for store in userdata {
        store(&ctx);
      }
      let flux_obj = Object::new(ctx.clone()).unwrap();

      #[cfg(feature = "http")]
      crate::standards_plugins::http::init_http(&ctx);
      crate::standards_plugins::time::init(&ctx);
      #[cfg(feature = "http")]
      crate::standards_plugins::fetch::init_fetch(&ctx);
      crate::standards_plugins::console::init_console(&ctx);
      crate::standards_plugins::navigator::init_navigator(&ctx);
      crate::standards_plugins::share::init_share(&ctx);
      events::init(&ctx);
      flux_obj.set("version", env!("FLUX_VERSION")).expect("failed to set Flux.version");
      crate::ore_plugins::availability::init(&ctx, &flux_obj);
      crate::ore_plugins::permission::init(&ctx, &flux_obj);
      crate::standards_plugins::headers::init_headers(&ctx);
      crate::standards_plugins::request::init_request(&ctx);
      crate::standards_plugins::response::init_response(&ctx);
      crate::standards_plugins::text::init_text(&ctx);
      crate::standards_plugins::streams::init_streams(&ctx);
      #[cfg(feature = "http")]
      crate::standards_plugins::websocket::init_websocket(&ctx);
      crate::standards_plugins::abort::init_abort(&ctx);
      crate::standards_plugins::crypto::init_crypto(&ctx);
      remove_array_buffer_transfer(&ctx);

      ctx.globals().set("Flux", flux_obj).unwrap();

      for setup in setups {
        setup(ctx.clone());
      }
      // After the whole chain: the gui seam and an embedder's modules list
      // (and the embedder excludes) as plugins, and the test host's
      // sessions set their availability and permission overrides.
      define_capabilities(&ctx).expect("failed to define Flux.capabilities");
      crate::ore_plugins::availability::emit(&ctx).expect("emit the availability snapshot");
      crate::ore_plugins::permission::emit(&ctx).expect("emit the permission map");
    })
    .await;

  // The memory limit and the interrupt handler are applied last: the startup
  // above (the built-in globals and the embedder's setups) is init-time work
  // that must run to completion. So a limit below the startup's own heap, or
  // a flag that is already set (an isolate killed while it starts), fails
  // only what the engine is given to run. The startup's heap still counts
  // toward the limit: QuickJS limits the runtime's total heap.
  if let Some(limit) = memory_limit {
    runtime.set_memory_limit(limit).await;
  }
  if let Some(flag) = interrupt {
    runtime.set_interrupt_handler(Some(Box::new(move || flag.load(Ordering::Relaxed)))).await;
  }

  (runtime, context, pending, rejections)
}

/// `ArrayBuffer.prototype.transfer` and friends come off the prototype:
/// flux:isolate's planned `transfer()` is the vocabulary for moving bytes
/// between runtimes (okf/backlog/isolate-transfer-and-abort.md), and one
/// word for it is enough. (The removal began as a soundness fix - the
/// quickjs-ng we used to vendor corrupted the heap on `transfer` of an
/// externally backed buffer - which the rquickjs 0.14 bump resolved; see
/// okf/upstream/quickjs-ng-transfer-external-buffers.md.)
fn remove_array_buffer_transfer(ctx: &Ctx<'_>) {
  let Ok(ab) = ctx.globals().get::<_, Object>("ArrayBuffer") else { return };
  let Ok(proto) = ab.get::<_, Object>("prototype") else { return };
  for name in ["transfer", "transferToImmutable", "transferToFixedLength"] {
    let _ = proto.remove(name);
  }
}

/// Feature names this flux build provides, the seed of every engine's
/// `Flux.capabilities` (see `CapabilityRegistry`; the gui seam and an
/// embedder list theirs on top, and the app's exclusions come off it).
/// JS branches on availability (`Flux.capabilities.includes("subprocess")`)
/// rather than on the OS. A conditionally-compiled feature sits under its
/// own cfg, so it only appears when actually present: `ktx2` says
/// flux:image's transcodeTexture and encodeTexture work here (they throw on
/// a build without it), `test` that flux:test can be imported, `wasm` that
/// flux:wasm can be. `wasm-native` (flux:wasm compiles modules to machine
/// code here, the wasmtime lane, instead of interpreting them) is the one
/// entry no cfg of this crate decides: the `wasm-native` feature resolves to
/// wasmi on a target without a backend, so `base_capabilities` reads the
/// lane forge was built with. Whether a platform facility (opening a link,
/// the share sheet, haptics, ...) is there now is no build fact and not
/// listed here: `Flux.availability` reports it (`ore_plugins/availability`).
const BASE_CAPABILITIES: &[&str] = &[
  "sqlite",
  "fs",
  #[cfg(feature = "http")]
  "http",
  #[cfg(feature = "tls")]
  "tls",
  #[cfg(feature = "p2p")]
  "p2p",
  #[cfg(feature = "p2p-relay")]
  "p2p-relay",
  "process",
  "path",
  "subprocess",
  "svg",
  "image",
  #[cfg(feature = "wasm")]
  "wasm",
  "isolate",
  "tty",
  #[cfg(feature = "ktx2")]
  "ktx2",
  #[cfg(feature = "test")]
  "test",
];

/// The capabilities every flux build provides (see `BASE_CAPABILITIES`);
/// the gui plugin set and an embedder add theirs on top.
pub fn base_capabilities() -> Vec<&'static str> {
  #[allow(unused_mut)]
  let mut caps = BASE_CAPABILITIES.to_vec();
  #[cfg(feature = "wasm")]
  if forge::wasm::LANE == forge::wasm::Lane::Native {
    caps.push("wasm-native");
  }
  caps
}

/// This engine's capability names: the listed ones in registration order,
/// without duplicates, and the ones the app's project declared it does not
/// use. `Flux.capabilities` is built from it once, after the plugin chain,
/// and is frozen from then on.
#[derive(JsLifetime)]
struct CapabilityRegistry(#[qjs(skip_trace)] RefCell<Capabilities>);

#[derive(Default)]
struct Capabilities {
  listed: Vec<String>,
  excluded: HashSet<String>,
  // Set once `Flux.capabilities` is defined: a later change reaches no JS.
  frozen: bool,
}

impl CapabilityRegistry {
  /// A registry listing what every flux build provides.
  fn seeded() -> CapabilityRegistry {
    let listed = base_capabilities().into_iter().map(str::to_string).collect();
    CapabilityRegistry(RefCell::new(Capabilities { listed, ..Capabilities::default() }))
  }
}

/// The registry's state for a change by `api`, or None (with a warning)
/// once `Flux.capabilities` is frozen or when the engine has no registry.
fn change_capabilities<R>(
  ctx: &Ctx<'_>,
  api: &str,
  name: &str,
  change: impl FnOnce(&mut Capabilities) -> R,
) -> Option<R> {
  let Some(registry) = ctx.userdata::<CapabilityRegistry>() else {
    log::warn!("[flux] {api} {name}: no capability registry in this engine");
    return None;
  };
  let mut capabilities = registry.0.borrow_mut();
  if capabilities.frozen {
    log::warn!("[flux] {api} {name}: Flux.capabilities is already frozen; run it as a plugin");
    return None;
  }
  Some(change(&mut capabilities))
}

/// List `name` in `Flux.capabilities`, for a module registered beside the
/// built-ins (the gui plugin set, an embedder's own modules): availability
/// checks stay uniform, `Flux.capabilities.includes(name)` whoever provides
/// it. A name already listed stays where it is. Runs as a plugin: the list
/// is frozen after the plugin chain.
pub fn add_capability(ctx: &Ctx<'_>, name: &str) {
  change_capabilities(ctx, "add_capability", name, |capabilities| {
    if !capabilities.listed.iter().any(|listed| listed == name) {
      capabilities.listed.push(name.to_string());
    }
  });
}

/// Leave `name` out of `Flux.capabilities` whoever lists it: the app's
/// project declared it does not use it (the manifest's `uses`). The module
/// still imports; a plugin that must refuse its calls reads
/// `capability_excluded`. Runs as a plugin, like `add_capability`.
pub fn exclude_capability(ctx: &Ctx<'_>, name: &str) {
  change_capabilities(ctx, "exclude_capability", name, |capabilities| {
    capabilities.excluded.insert(name.to_string());
  });
}

/// Whether the app's project declared it does not use `name` (see
/// `exclude_capability`).
pub fn capability_excluded(ctx: &Ctx<'_>, name: &str) -> bool {
  ctx.userdata::<CapabilityRegistry>().is_some_and(|registry| registry.0.borrow().excluded.contains(name))
}

/// The names `Flux.capabilities` holds: the listed ones, in registration
/// order, minus the excluded ones (for an embedder's control surface).
pub fn listed_capabilities(ctx: &Ctx<'_>) -> Vec<String> {
  let Some(registry) = ctx.userdata::<CapabilityRegistry>() else {
    return Vec::new();
  };
  let capabilities = registry.0.borrow();
  capabilities.listed.iter().filter(|name| !capabilities.excluded.contains(*name)).cloned().collect()
}

/// Define `Flux.capabilities` as a frozen array on a non-writable,
/// non-configurable property, then freeze `Flux` itself: the registry
/// takes no change after this.
fn define_capabilities(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
  let array = Array::new(ctx.clone())?;
  for (i, name) in listed_capabilities(ctx).into_iter().enumerate() {
    array.set(i, name)?;
  }
  if let Some(registry) = ctx.userdata::<CapabilityRegistry>() {
    registry.0.borrow_mut().frozen = true;
  }
  let object: Object = ctx.globals().get("Object")?;
  let freeze: Function = object.get("freeze")?;
  freeze.call::<_, ()>((array.clone(),))?;
  let flux: Object = ctx.globals().get("Flux")?;
  flux.prop("capabilities", Property::from(array).enumerable())?;
  freeze.call::<_, ()>((flux,))
}
