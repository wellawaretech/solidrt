// flux - run a JS source file via FluxEngine, or (`--test`, in a build
// with the `test` feature) run it as a test file: every test it registers
// in an engine of its own, each result one record line on stdout.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use flux::{FluxEngine, FluxEngineBuilder, LogLevel, ModuleCode, ProcessArgs, ProcessExit};

// Through forge::tty so a line breaks correctly while the terminal is in raw
// mode (flux:tty setRawMode), where a bare "\n" would not return the carriage.
fn log_fn(_level: LogLevel, msg: &str) {
  forge::tty::write_line(msg);
}

// The builder of one engine. `exit` is whether the program may end the
// process (`exit()` from flux:process): a plain run is the process, so yes;
// a test engine is one of a file's many, and ending the process would end
// the file's run without its closing line, so a test host holds it back as
// a windowed app's engine and the isolates do.
fn engine_builder(base: &Path, argv: &[String], exit: bool) -> FluxEngineBuilder {
  let base = base.to_path_buf();
  let builder = FluxEngine::builder().logger(log_fn).userdata(ProcessArgs(argv.to_vec())).isolate_resolver(move |id| {
    // Bytecode first, like the lattice resolver: a compiled bundle dir
    // ships isolates/<id>.fluxbc, a source layout isolates/<id>.js.
    let dir = base.join("isolates");
    if let Ok(bytes) = std::fs::read(dir.join(format!("{id}.fluxbc"))) {
      return Ok(ModuleCode::Bytecode(bytes));
    }
    let file = dir.join(format!("{id}.js"));
    std::fs::read_to_string(&file)
      .map(ModuleCode::Source)
      .map_err(|e| format!("isolate '{id}': cannot read {}: {e}", file.display()))
  });
  if exit {
    builder.userdata(ProcessExit)
  } else {
    builder
  }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  forge::process::return_large_allocations();
  // The first argument is the script path ("-" or absent: stdin); everything
  // after it is the program's argument vector, forwarded to JS through
  // flux:process (which exposes app arguments only, no executable/script).
  // Raw terminal mode is process-wide state the terminal keeps after we are
  // gone: put it back on every way out, the panic path included.
  let default_panic = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
    forge::tty::restore();
    default_panic(info);
  }));

  let mut args = std::env::args().skip(1).peekable();
  #[cfg(feature = "test")]
  let test = test_options(&mut args);
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

  #[cfg(feature = "test")]
  if let Some((options, sandbox)) = test {
    // The sandbox is the working directory, emptied ahead of every engine
    // (okf/done/test-harness.md, D38): a test starts from no stored state,
    // whatever the tests before it wrote, and writes relative paths.
    if let Some(dir) = &sandbox {
      std::env::set_current_dir(dir).unwrap_or_else(|e| {
        eprintln!("flux: cannot enter the sandbox {}: {e}", dir.display());
        std::process::exit(2);
      });
    }
    let builder = || {
      if let Some(dir) = &sandbox {
        empty_dir(dir);
      }
      engine_builder(&base, &argv, false)
    };
    let passed = flux::test::run_file(ModuleCode::Source(source), options, builder, |record| {
      forge::tty::write_line(&record.line())
    })
    .await;
    std::process::exit(if passed { 0 } else { 1 });
  }

  // Any uncaught error (module-level throw, unhandled rejection, throw out of
  // a timer or event callback) fails the run: the engine reports and keeps
  // going, the binary turns that into a nonzero exit like node and bun do.
  let failed = Arc::new(AtomicBool::new(false));
  let mark_failed = failed.clone();
  let engine =
    engine_builder(&base, &argv, true).on_uncaught(move |_| mark_failed.store(true, Ordering::Relaxed)).build();
  engine.eval_source(&source).await;
  forge::tty::restore();
  if failed.load(Ordering::Relaxed) {
    std::process::exit(1);
  }
}

// Remove everything in `dir`; the folder itself stays, as it is the
// working directory.
#[cfg(feature = "test")]
fn empty_dir(dir: &Path) {
  let Ok(entries) = std::fs::read_dir(dir) else { return };
  for entry in entries.flatten() {
    let path = entry.path();
    let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
    if let Err(e) = removed {
      eprintln!("flux: could not remove {} from the sandbox: {e}", path.display());
    }
  }
}

// The test flags, which come ahead of the script path: `--test`, then
// `--filter <text>`, `--seed <n>` and `--data-root <dir>` in any order.
// None without `--test`. The data root is the sandbox the tests run in.
#[cfg(feature = "test")]
fn test_options(
  args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
) -> Option<(flux::test::RunOptions, Option<PathBuf>)> {
  args.next_if(|arg| arg == "--test")?;
  let mut options = flux::test::RunOptions::default();
  let mut sandbox = None;
  while let Some(flag) = args.next_if(|arg| arg == "--filter" || arg == "--seed" || arg == "--data-root") {
    let Some(value) = args.next() else {
      eprintln!("flux: {flag} requires a value");
      std::process::exit(2);
    };
    match flag.as_str() {
      "--filter" => options.filter = Some(value),
      "--data-root" => sandbox = Some(PathBuf::from(value)),
      _ => {
        options.seed = value.parse().unwrap_or_else(|_| {
          eprintln!("flux: --seed value must be a non-negative integer");
          std::process::exit(2);
        })
      }
    }
  }
  Some((options, sandbox))
}
