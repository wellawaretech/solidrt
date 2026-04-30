use qjsrt::{JsEngine, LogLevel};

fn log_fn(_level: LogLevel, msg: &str) {
    println!("[log] {msg}");
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let engine = JsEngine::builder().logger(log_fn).build();

    engine.eval_source(r#"console.log("Hello, World!")"#).await;
}
