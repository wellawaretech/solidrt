mod engine;
mod timer;

pub use engine::JsEngine;

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
