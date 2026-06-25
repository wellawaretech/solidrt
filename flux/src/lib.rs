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

#[cfg(feature = "compile")]
pub fn compile_source(source: &str, module_name: &str) -> Vec<u8> {
  use plugins::modules;
  use rquickjs::loader::{BuiltinResolver, ModuleLoader};

  let rt = Runtime::new().expect("failed to create QuickJS runtime");

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
  rt.set_loader(resolver, loader);

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
