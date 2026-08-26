//! Engine-free terminal core: whether stdin is a terminal, cooked-mode line
//! input from it, and raw writes to stdout. The marshalling layer
//! (`flux/src/forge_plugins/tty.rs`) owns the event-bus wiring and forwards
//! to the pieces here.

use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

/// Whether stdin is a terminal (as opposed to a pipe, a file, or nothing).
pub fn is_terminal() -> bool {
  std::io::stdin().is_terminal()
}

/// One delivery from the stdin reader: a line (newline stripped) or the end
/// of the stream.
pub enum Input {
  Line(String),
  Eof,
}

static TAKEN: AtomicBool = AtomicBool::new(false);

/// Start reading stdin line by line and hand back the receiving end. stdin is
/// one stream per process, so only the first call gets it; later calls return
/// `None`. The reader is a plain OS thread, deliberately not tokio's blocking
/// pool: a runtime being dropped waits for its blocking tasks, and a read
/// parked on stdin would then hang the process at exit, while a detached
/// thread just dies with it. Lines typed while nobody receives queue in the
/// channel.
pub fn open_lines() -> Option<mpsc::UnboundedReceiver<Input>> {
  if TAKEN.swap(true, Ordering::SeqCst) {
    return None;
  }
  let (tx, rx) = mpsc::unbounded_channel();
  std::thread::Builder::new()
    .name("stdin-lines".into())
    .spawn(move || {
      let stdin = std::io::stdin();
      let mut line = String::new();
      loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
          Ok(0) | Err(_) => {
            let _ = tx.send(Input::Eof);
            break;
          }
          Ok(_) => {
            let text = line.strip_suffix('\n').unwrap_or(&line);
            let text = text.strip_suffix('\r').unwrap_or(text);
            if tx.send(Input::Line(text.to_string())).is_err() {
              break;
            }
          }
        }
      }
    })
    .expect("spawn stdin reader thread");
  Some(rx)
}

/// Write `text` to stdout as is (no newline appended) and flush, so a prompt
/// shows before the line it waits for.
pub fn write(text: &str) -> std::io::Result<()> {
  let mut out = std::io::stdout().lock();
  out.write_all(text.as_bytes())?;
  out.flush()
}
