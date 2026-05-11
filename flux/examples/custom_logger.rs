use flux::{FluxEngine, LogLevel};

fn log_fn(_level: LogLevel, msg: &str) {
  println!("[log] {msg}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let engine = FluxEngine::builder().logger(log_fn).build();

  engine.eval_source(r#"console.log("Hello, World!")"#).await;
}
