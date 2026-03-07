mod engine;

pub use engine::JsEngine;

use std::time::Duration;

#[derive(Clone, Copy)]
pub struct RunOptions {
    pub timeout: Option<Duration>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self { timeout: None }
    }
}

pub fn run(code: &str, opts: Option<RunOptions>) -> String {
    let opts = opts.unwrap_or_default();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    let engine = JsEngine::new();
    rt.block_on(async {
        let result = engine.eval(code).await;
        match opts.timeout {
            Some(d) => { let _ = tokio::time::timeout(d, engine.wait_idle()).await; }
            None => engine.wait_idle().await,
        }
        engine.shutdown().await;
        result
    })
}
