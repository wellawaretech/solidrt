mod engine;
mod timer;

pub use engine::JsEngine;

use std::time::Duration;

pub fn run(code: &str, timeout: Option<Duration>) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let engine = JsEngine::new();
    rt.block_on(async {
        engine.eval_module(code).await;
        match timeout {
            Some(d) => { let _ = tokio::time::timeout(d, engine.wait_idle()).await; }
            None => engine.wait_idle().await,
        }
        engine.shutdown().await;
    })
}

pub fn run_script(code: &str, timeout: Option<Duration>) -> String {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let engine = JsEngine::new();
    rt.block_on(async {
        if let Err(e) = engine.eval(code).await {
            engine.shutdown().await;
            return e;
        }
        match timeout {
            Some(d) => { let _ = tokio::time::timeout(d, engine.wait_idle()).await; }
            None => engine.wait_idle().await,
        }
        let result = engine.stringify_last().await;
        engine.shutdown().await;
        result
    })
}
