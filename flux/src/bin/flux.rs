// flux - run a JS source file via FluxEngine

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use flux::{FluxEngine, LogLevel, ModuleCode, ProcessArgs};

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

  // `isolate("worker")` is `<entry dir>/isolates/worker.js`; a stdin script
  // has no directory, so its isolates live under the working directory.
  let base: PathBuf = match path.as_deref() {
    Some("-") | None => PathBuf::from("."),
    Some(p) => Path::new(p).parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf(),
  };
  // Any uncaught error (module-level throw, unhandled rejection, throw out of
  // a timer or event callback) fails the run: the engine reports and keeps
  // going, the binary turns that into a nonzero exit like node and bun do.
  let failed = Arc::new(AtomicBool::new(false));
  let mark_failed = failed.clone();
  let engine = FluxEngine::builder()
    .logger(log_fn)
    .userdata(ProcessArgs(argv))
    .on_uncaught(move |_| mark_failed.store(true, Ordering::Relaxed))
    .isolate_resolver(move |id| {
      // Bytecode first, like the lattice resolver: a compiled bundle dir
      // ships isolates/<id>.bin, a source layout isolates/<id>.js.
      let dir = base.join("isolates");
      if let Ok(bytes) = std::fs::read(dir.join(format!("{id}.bin"))) {
        return Ok(ModuleCode::Bytecode(bytes));
      }
      let file = dir.join(format!("{id}.js"));
      std::fs::read_to_string(&file)
        .map(ModuleCode::Source)
        .map_err(|e| format!("isolate '{id}': cannot read {}: {e}", file.display()))
    })
    .build();
  engine.eval_source(&source).await;
  if failed.load(Ordering::Relaxed) {
    std::process::exit(1);
  }
}
