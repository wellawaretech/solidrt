// Stand-alone entry point - compile JS from stdin to bytecode on stdout

struct StderrLogger;
impl log::Log for StderrLogger {
  fn enabled(&self, _: &log::Metadata) -> bool { true }
  fn log(&self, record: &log::Record) { eprintln!("{}", record.args()); }
  fn flush(&self) {}
}
static STDERR_LOGGER: StderrLogger = StderrLogger;

fn main() {
  log::set_logger(&STDERR_LOGGER).ok();
  log::set_max_level(log::LevelFilter::Error);

  let mut source = String::new();
  std::io::Read::read_to_string(&mut std::io::stdin(), &mut source).unwrap_or_else(|e| {
    log::error!("[flux] error: failed to read stdin: {e}");
    std::process::exit(1);
  });

  let bytecode = flux::compile_source(&source, "stdin");
  std::io::Write::write_all(&mut std::io::stdout(), &bytecode).unwrap_or_else(|e| {
    log::error!("[flux] error: failed to write stdout: {e}");
    std::process::exit(1);
  });
}
