mod engine;
mod logger;
pub(crate) mod pending;
mod plugins;

pub use engine::{JsEngine, JsEngineBuilder, JsSession, ShutdownHooks, on_shutdown};
pub use logger::LogLevel;
pub use plugins::events::emit_event;
pub use rquickjs;

#[cfg(feature = "compile")]
use rquickjs::{CatchResultExt, Context, Module, Runtime, WriteOptions, WriteOptionsEndianness};

#[cfg(feature = "compile")]
pub fn compile_source(source: &str, module_name: &str) -> Vec<u8> {
    let rt = Runtime::new().expect("failed to create QuickJS runtime");
    let ctx = Context::full(&rt).expect("failed to create QuickJS context");

    let result = ctx.with(|ctx| {
        let module = Module::declare(ctx.clone(), module_name, source)
            .catch(&ctx)
            .map_err(|e| format!("failed to compile '{module_name}': {e}"))?;

        module
            .write(WriteOptions {
                endianness: WriteOptionsEndianness::Little,
                ..Default::default()
            })
            .catch(&ctx)
            .map_err(|e| format!("failed to write bytecode: {e}"))
    });

    result.unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    })
}
