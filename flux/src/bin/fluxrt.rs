// fluxrt - self-contained flux runtime; runs bytecode appended to this binary

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use flux::{FluxEngine, LogLevel, ProcessArgs};

const MAGIC: &[u8; 8] = b"FLUXRT\x88\x44";
const TRAILER_LEN: usize = 16; // u64 offset (8 bytes) + magic (8 bytes)

fn log_fn(_level: LogLevel, msg: &str) {
  println!("{msg}");
}

fn load_embedded_bytecode() -> Option<Vec<u8>> {
  let exe = std::env::current_exe().ok()?;
  let data = std::fs::read(&exe).ok()?;
  if data.len() < TRAILER_LEN {
    return None;
  }
  let magic_start = data.len() - 8;
  if &data[magic_start..] != MAGIC {
    return None;
  }
  let offset_start = data.len() - TRAILER_LEN;
  let offset = u64::from_le_bytes(data[offset_start..magic_start].try_into().ok()?) as usize;
  if offset >= offset_start {
    return None;
  }
  Some(data[offset..offset_start].to_vec())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
  let bytecode = load_embedded_bytecode().unwrap_or_else(|| {
    eprintln!("fluxrt: no embedded bytecode found");
    std::process::exit(1);
  });

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
    .build();
  engine.eval(bytecode).await;
  if failed.load(Ordering::Relaxed) {
    std::process::exit(1);
  }
}
