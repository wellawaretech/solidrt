#[cfg(feature = "gui")]
pub mod alloy_plugins;
mod engine;
mod forge_plugins;
mod logger;
pub(crate) mod pending;
mod plugins;
mod standards_plugins;
#[cfg(feature = "test")]
mod test_plugins;

#[cfg(test)]
mod tests;

#[cfg(feature = "gui")]
pub use alloy_plugins as gui;
// The gui bindings hand out alloy's types (`gui::alloy_context`), so a module
// names them through flux rather than through a dependency of its own.
#[cfg(feature = "gui")]
pub use alloy;
#[cfg(feature = "test")]
pub use test_plugins::host as test;

pub use engine::{
  on_shutdown, resolve_isolate_from_assets, EngineConfig, ExecHandle, FluxEngine, FluxEngineBuilder, IsolateResolver,
  ModuleCode, ShutdownHooks, ENTRY_MODULE,
};
#[cfg(feature = "http")]
pub use forge::fetch::{do_fetch, ResponseData};
pub use forge::process::{arch, platform};
pub use forge::seek::{SeekableRead, SeekableReader};
pub use forge_plugins::process::{ProcessArgs, ProcessExit};
pub use logger::{report_uncaught, CtxLogger, LogLevel, Logger};
pub use pending::{describe_in_flight, hold_engine, in_flight, settled, Hold};
pub use plugins::add_capability;
pub use plugins::events::{emit_event, emit_sticky, has_listeners, register_listener, sticky_cached};
pub use plugins::js_error::JsResult;
// The marshalling toolkit a module over flux uses as its plugins do (a
// custom runtime's `runtime:*` module, docs/runtime "Native code"): the
// promise-with-hold shape of every async binding, the bytes of a JS buffer,
// the Rust-backed async iterator, and an options object's fields.
pub use plugins::marshal::{
  attach_async_iterator, bytes_of, elements_mut_of, elements_of, iter_result, string_opt, with_in_flight,
  with_standing, CopyBytes, OptArg, Step,
};
pub use plugins::seekable::{SeekableOpener, SeekableSource};
pub use rquickjs;
// The async runtime the engine's futures run on, for a module's own channels,
// blocking work and timers (`tokio::task::spawn_blocking`, `sync::oneshot`),
// named through flux like rquickjs rather than pinned again by the module.
pub use standards_plugins::body::{attach_body, JsBytes, JsonValue};
#[cfg(feature = "http")]
pub use standards_plugins::fetch::{request_body_from_value, JsResponseData};
pub use standards_plugins::headers::header_pairs_from_init;
pub use standards_plugins::random::{seed_random, DEFAULT_SEED};
pub use standards_plugins::time::{
  advance_virtual_time, advance_virtual_time_to_now, freeze_wall, idle_due, install_idle_periods, install_virtual_time,
  set_virtual_now_source, timeline_now_ms, timer_due, Timeline,
};
pub use tokio;

/// Feature names this build provides, as surfaced to JS via `Flux.capabilities`.
/// Native callers (e.g. a dev client introspecting the runtime) get the same
/// list without a JS context.
pub fn capabilities() -> Vec<&'static str> {
  #[allow(unused_mut)]
  let mut caps = plugins::base_capabilities();
  #[cfg(feature = "gui")]
  caps.extend_from_slice(alloy_plugins::GUI_CAPABILITIES);
  #[cfg(feature = "gui")]
  if alloy::dialogs_available() {
    caps.push(alloy_plugins::DIALOG_CAPABILITY);
  }
  caps
}

use rquickjs::{CatchResultExt, Context, Module, Runtime, WriteOptions, WriteOptionsEndianness};

/// The module name of the fingerprint's probe: an atom in its bytes, so fixed.
const ENGINE_PROBE_MODULE: &str = "engine";

/// What decides whether bytecode loads in this engine: the bytes it writes
/// for a fixed empty module, as hex. Two builds with the same fingerprint
/// share a bytecode format (the version it stamps, the layout it writes).
/// `solidrt --engine` and `fluxc --engine` print it, and `sol pack` compares
/// its compiler's with that of the runtime it packs for, since a runtime
/// refuses bytecode from another engine.
pub fn engine_fingerprint() -> Result<String, String> {
  let rt = Runtime::new().map_err(|e| format!("failed to create QuickJS runtime: {e}"))?;
  let ctx = Context::full(&rt).map_err(|e| format!("failed to create QuickJS context: {e}"))?;
  let bytes = ctx.with(|ctx| {
    Module::declare(ctx.clone(), ENGINE_PROBE_MODULE, "")
      .and_then(|module| {
        module.write(WriteOptions { endianness: WriteOptionsEndianness::Little, ..Default::default() })
      })
      .catch(&ctx)
      .map_err(|e| format!("failed to write the probe module: {e}"))
  })?;
  Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

// Bundles arrive here with every real source inlined; the only remaining
// imports are the modules the runtime provides, which the bundler left
// external: the platform's `flux:*` and `sol:*` capability modules and a
// custom runtime's `runtime:*` ones. Compilation only records each as an
// external reference (named imports are resolved at runtime link, not here),
// so the compiler links none of them and needs no per-module enumeration: any
// specifier under one of the schemes resolves to an empty placeholder module.
// The runner (lattice/fluxrt) is the authority on which actually exist; a
// bogus name fails at startup, not here.
#[cfg(feature = "compile")]
const RUNTIME_MODULE_SCHEMES: [&str; 3] = ["flux:", "sol:", "runtime:"];

#[cfg(feature = "compile")]
struct ExternResolver;

#[cfg(feature = "compile")]
impl rquickjs::loader::Resolver for ExternResolver {
  fn resolve<'js>(
    &mut self,
    _ctx: &rquickjs::Ctx<'js>,
    base: &str,
    name: &str,
    _attrs: Option<rquickjs::loader::ImportAttributes<'js>>,
  ) -> rquickjs::Result<String> {
    if RUNTIME_MODULE_SCHEMES.iter().any(|scheme| name.starts_with(scheme)) {
      Ok(name.to_string())
    } else {
      Err(rquickjs::Error::new_resolving(base.to_string(), name.to_string()))
    }
  }
}

#[cfg(feature = "compile")]
struct ExternLoader;

#[cfg(feature = "compile")]
impl rquickjs::loader::Loader for ExternLoader {
  fn load<'js>(
    &mut self,
    ctx: &rquickjs::Ctx<'js>,
    name: &str,
    _attrs: Option<rquickjs::loader::ImportAttributes<'js>>,
  ) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
    Module::declare(ctx.clone(), name, "")
  }
}

#[cfg(feature = "compile")]
pub fn compile_source(source: &str, module_name: &str) -> Vec<u8> {
  let rt = Runtime::new().expect("failed to create QuickJS runtime");

  rt.set_loader(ExternResolver, ExternLoader);

  let ctx = Context::full(&rt).expect("failed to create QuickJS context");

  let result = ctx.with(|ctx| {
    let module = Module::declare(ctx.clone(), module_name, source)
      .catch(&ctx)
      .map_err(|e| format!("failed to compile '{module_name}': {e}"))?;

    module
      .write(WriteOptions { endianness: WriteOptionsEndianness::Little, ..Default::default() })
      .catch(&ctx)
      .map_err(|e| format!("failed to write bytecode: {e}"))
  });

  result.unwrap_or_else(|e| {
    log::error!("[flux] error: {e}");
    std::process::exit(1);
  })
}
