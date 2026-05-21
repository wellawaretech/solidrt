// flux - run a JS source file via FluxEngine

use flux::{FluxEngine, LogLevel};

fn log_fn(_level: LogLevel, msg: &str) {
  println!("{msg}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let mut args = std::env::args().skip(1);
  let path = match args.next() {
    Some(p) => p,
    None => {
      eprintln!("usage: flux <file.js>");
      std::process::exit(1);
    }
  };

  let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
    eprintln!("flux: failed to read {path}: {e}");
    std::process::exit(1);
  });

  let engine = FluxEngine::builder().logger(log_fn).build();
  engine.eval_source(&source).await;
}