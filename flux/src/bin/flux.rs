// flux - run a JS source file via FluxEngine

use flux::{FluxEngine, LogLevel, ProcessArgs};

fn log_fn(_level: LogLevel, msg: &str) {
  println!("{msg}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  // The first argument is the script path ("-" or absent: stdin); everything
  // after it is the program's argument vector, forwarded to JS through
  // flux:process (which exposes app arguments only, no executable/script).
  let mut args = std::env::args().skip(1);
  let path = args.next();
  let argv: Vec<String> = args.collect();

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

  let engine = FluxEngine::builder().logger(log_fn).userdata(ProcessArgs(argv)).build();
  engine.eval_source(&source).await;
}
