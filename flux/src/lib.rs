mod engine;
mod logger;
pub(crate) mod pending;
mod plugins;

#[cfg(feature = "gui")]
pub use plugins::gui;

pub use engine::{on_shutdown, ExecHandle, FluxEngine, FluxEngineBuilder, ShutdownHooks};
pub use forge::fetch::{do_fetch, ResponseData};
pub use forge::process::{arch, platform};
pub use logger::{report_uncaught, CtxLogger, LogLevel, Logger};
pub use plugins::js_error::JsResult;
pub use plugins::modules::events::{emit_event, has_listeners, register_listener};
pub use plugins::modules::process::ProcessArgs;
pub use plugins::standards::body::{attach_body, JsBytes, JsonValue};
pub use plugins::standards::fetch::JsResponseData;
pub use plugins::standards::time::Clock;
pub use rquickjs;

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
