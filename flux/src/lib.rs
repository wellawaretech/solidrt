mod console;
mod engine;
mod events;
mod io;
mod timer;

pub use engine::{JsEngine, JsEngineBuilder};
pub use rquickjs;

use rquickjs::{Context, Module, Runtime, WriteOptions, WriteOptionsEndianness};

#[cfg(feature = "script")]
use std::time::Duration;

pub fn run(code: &str) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let engine = JsEngine::new();
    rt.block_on(async {
        engine.eval(code).await;
        engine.shutdown().await;
    })
}

pub fn compile(input_path: &str, output_path: &str) {
    let source = std::fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("error: cannot read '{input_path}': {e}");
        std::process::exit(1);
    });

    let rt = Runtime::new().expect("failed to create QuickJS runtime");
    let ctx = Context::full(&rt).expect("failed to create QuickJS context");

    let bytecode = ctx.with(|ctx| {
        let module = Module::declare(ctx.clone(), input_path, source).unwrap_or_else(|e| {
            eprintln!("error: failed to compile '{input_path}': {e}");
            std::process::exit(1);
        });

        module
            .write(WriteOptions {
                endianness: WriteOptionsEndianness::Little,
                ..Default::default()
            })
            .unwrap_or_else(|e| {
                eprintln!("error: failed to write bytecode: {e}");
                std::process::exit(1);
            })
    });

    std::fs::write(output_path, &bytecode).unwrap_or_else(|e| {
        eprintln!("error: cannot write '{output_path}': {e}");
        std::process::exit(1);
    });

    println!("wrote {} bytes to {output_path}", bytecode.len());
}

pub fn run_bytecode(bytecode: Vec<u8>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let engine = JsEngine::new();
    rt.block_on(async {
        engine.eval_bytecode(bytecode).await;
        engine.shutdown().await;
    })
}

#[cfg(feature = "script")]
pub fn run_script(code: &str, timeout: Option<Duration>) -> String {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let engine = JsEngine::new();
    rt.block_on(async {
        let result = match timeout {
            Some(d) => match tokio::time::timeout(d, engine.eval_script(code)).await {
                Ok(Ok(val)) => val,
                Ok(Err(e)) => e,
                Err(_) => "error: timed out".into(),
            },
            None => match engine.eval_script(code).await {
                Ok(val) => val,
                Err(e) => e,
            },
        };
        engine.shutdown().await;
        result
    })
}
