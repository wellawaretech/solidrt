mod engine;
mod logger;
pub(crate) mod pending;
mod plugins;
mod standards_plugins;
mod forge_plugins;
#[cfg(feature = "gui")]
pub mod alloy_plugins;

#[cfg(test)]
mod tests;

#[cfg(feature = "gui")]
pub use alloy_plugins as gui;

pub use engine::{
  on_shutdown, resolve_isolate_from_assets, EngineConfig, ExecHandle, FluxEngine, FluxEngineBuilder, IsolateResolver,
  ModuleCode, ShutdownHooks,
};
pub use forge::fetch::{do_fetch, ResponseData};
pub use forge::process::{arch, platform};
pub use forge::seek::{SeekableRead, SeekableReader};
pub use logger::{report_uncaught, CtxLogger, LogLevel, Logger};
pub use plugins::js_error::JsResult;
pub use forge_plugins::events::{emit_event, emit_sticky, has_listeners, register_listener, sticky_cached};
pub use forge_plugins::process::ProcessArgs;
pub use plugins::seekable::{SeekableOpener, SeekableSource};
pub use standards_plugins::body::{attach_body, JsBytes, JsonValue};
pub use standards_plugins::fetch::{request_body_from_value, JsResponseData};
pub use standards_plugins::headers::header_pairs_from_init;
pub use standards_plugins::time::{advance_virtual_time, install_virtual_time, Timeline};
pub use rquickjs;

/// Feature names this build provides, as surfaced to JS via `Flux.capabilities`.
/// Native callers (e.g. a dev client introspecting the runtime) get the same
/// list without a JS context.
pub fn capabilities() -> Vec<&'static str> {
  #[allow(unused_mut)]
  let mut caps = plugins::BASE_CAPABILITIES.to_vec();
  #[cfg(feature = "gui")]
  caps.extend_from_slice(alloy_plugins::GUI_CAPABILITIES);
  caps
}

#[cfg(feature = "compile")]
use rquickjs::{CatchResultExt, Context, Module, Runtime, WriteOptions, WriteOptionsEndianness};

// Bundles arrive here with every real source inlined; the only remaining
// imports are the runtime-provided `flux:*` and `srt:*` capability modules that
// esbuild left external. Compilation only records each as an external reference
// (named imports are resolved at runtime link, not here), so the compiler links
// none of them and needs no per-module enumeration: any `flux:`/`srt:` specifier
// resolves to an empty placeholder module. The runner (lattice/fluxrt) is the
// authority on which actually exist; a bogus name fails at startup, not here.
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
    if name.starts_with("flux:") || name.starts_with("srt:") {
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
