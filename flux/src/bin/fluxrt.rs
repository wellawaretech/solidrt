// fluxrt - self-contained flux runtime; runs bytecode appended to this binary

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

  let argv: Vec<String> = std::env::args().collect();

  let engine = FluxEngine::builder().logger(log_fn).userdata(ProcessArgs(argv)).build();
  engine.eval(bytecode).await;
}
