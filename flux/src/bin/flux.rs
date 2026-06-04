// flux - run a JS source file via FluxEngine

use flux::{FluxEngine, LogLevel};

fn log_fn(_level: LogLevel, msg: &str) {
  println!("{msg}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let mut args = std::env::args().skip(1);
  let path = args.next();

  let source = match path.as_deref() {
    Some("-") | None => {
      let mut s = String::new();
      std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).unwrap_or_else(|e| {
        eprintln!("flux: failed to read stdin: {e}");
        std::process::exit(1);
      });
      s
    }
    Some(p) => std::fs::read_to_string(p).unwrap_or_else(|e| {
      eprintln!("flux: failed to read {p}: {e}");
      std::process::exit(1);
    }),
  };

  let engine = FluxEngine::builder().logger(log_fn).build();
  engine.eval_source(&source).await;
}
