// fluxrt - self-contained flux runtime; runs the payload appended to this binary

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use flux::{FluxEngine, LogLevel, ProcessArgs};

const MAGIC: &[u8; 8] = b"FLUXRT\x88\x44";

// Through forge::tty so a line breaks correctly while the terminal is in raw
// mode (flux:tty setRawMode), where a bare "\n" would not return the carriage.
fn log_fn(_level: LogLevel, msg: &str) {
  forge::tty::write_line(msg);
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  // The same section trailer the solidrt runner uses (forge::trailer; written
  // by packages/cli/src/packer.ts). A fluxrt payload carries only kind-2 file
  // sections: "bundle.bin" is the program, "isolates/<id>.bin" its isolate
  // modules. The program is read once here; isolates stay in place and
  // resolve by ranged reads through the assets mount, exactly as in solidrt.
  let Some(trailer) = forge::trailer::read_own(MAGIC) else {
    eprintln!("fluxrt: no embedded payload found");
    std::process::exit(1);
  };
  let main = trailer
    .sections
    .iter()
    .find(|s| s.kind == forge::trailer::SECTION_FILE && s.name == "bundle.bin")
    .and_then(|s| trailer.section_bytes(s).ok())
    .unwrap_or_else(|| {
      eprintln!("fluxrt: no embedded payload found");
      std::process::exit(1);
    });
  let index = trailer.file_index();
  forge::fs::set_assets_base(Some(forge::fs::AssetsBase::Packed { exe: trailer.exe, index }));

  // Everything after the executable is the program's argument vector,
  // forwarded to JS through flux:process (app arguments only).
  let argv: Vec<String> = std::env::args().skip(1).collect();

  // Any uncaught error fails the run with a nonzero exit (see bin/flux.rs).
  let failed = Arc::new(AtomicBool::new(false));
  let mark_failed = failed.clone();
  let engine = FluxEngine::builder()
    .logger(log_fn)
    .userdata(ProcessArgs(argv))
    .on_uncaught(move |_| mark_failed.store(true, Ordering::Relaxed))
    .isolate_resolver(flux::resolve_isolate_from_assets)
    .build();
  engine.eval(main).await;
  if failed.load(Ordering::Relaxed) {
    std::process::exit(1);
  }
}
