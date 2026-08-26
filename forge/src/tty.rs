//! Engine-free terminal core: whether stdin is a terminal, input from it
//! (cooked lines, or keys in raw mode), raw terminal mode, and writes to
//! stdout that break lines correctly in either mode. The marshalling layer
//! (`flux/src/forge_plugins/tty.rs`) owns the event-bus wiring and forwards
//! to the pieces here.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

/// Whether stdin is a terminal this process can use: a terminal (not a pipe,
/// a file, or nothing) and, on unix, one whose foreground job we are. A job
/// backgrounded from an interactive shell (`cmd &`) keeps the terminal as its
/// stdin, but job control stops it the moment it changes terminal settings
/// (SIGTTOU) or reads (SIGTTIN); a terminal we would be stopped for touching
/// is reported as none. Windows has no job control.
pub fn is_terminal() -> bool {
  std::io::stdin().is_terminal() && is_foreground()
}

#[cfg(unix)]
fn is_foreground() -> bool {
  rustix::termios::tcgetpgrp(std::io::stdin()).is_ok_and(|group| group == rustix::process::getpgrp())
}

#[cfg(not(unix))]
fn is_foreground() -> bool {
  true
}

/// One key press in raw mode. `name` follows Node's keypress names ("return",
/// "backspace", "tab", "up", "escape", "f1", a lowercase letter, "space");
/// `ch` is the character typed, with its case, for printable keys.
pub struct Key {
  pub name: String,
  pub ch: Option<char>,
  pub ctrl: bool,
  pub meta: bool,
  pub shift: bool,
}

/// One delivery from the stdin reader: a cooked line (newline stripped), a
/// key (raw mode), or the end of the stream.
pub enum Input {
  Line(String),
  Key(Key),
  Eof,
}

static TAKEN: AtomicBool = AtomicBool::new(false);
static RAW: AtomicBool = AtomicBool::new(false);

/// Start reading stdin and hand back the receiving end. stdin is one stream
/// per process, so only the first call gets it; later calls return `None`.
/// Each read is a cooked line or, while raw mode is on, one key: a mode
/// change applies from the next read. The reader is a plain OS thread,
/// deliberately not tokio's blocking pool: a runtime being dropped waits for
/// its blocking tasks, and a read parked on stdin would then hang the process
/// at exit, while a detached thread just dies with it. Input arriving while
/// nobody receives queues in the channel.
pub fn open_input() -> Option<mpsc::UnboundedReceiver<Input>> {
  if TAKEN.swap(true, Ordering::SeqCst) {
    return None;
  }
  let (tx, rx) = mpsc::unbounded_channel();
  std::thread::Builder::new()
    .name("stdin-input".into())
    .spawn(move || {
      let stdin = std::io::stdin();
      let mut line = String::new();
      loop {
        let input = if RAW.load(Ordering::SeqCst) { read_key() } else { Some(read_line(&stdin, &mut line)) };
        let Some(input) = input else { continue };
        let eof = matches!(input, Input::Eof);
        if tx.send(input).is_err() || eof {
          break;
        }
      }
    })
    .expect("spawn stdin reader thread");
  Some(rx)
}

fn read_line(stdin: &std::io::Stdin, line: &mut String) -> Input {
  line.clear();
  match stdin.lock().read_line(line) {
    Ok(0) | Err(_) => Input::Eof,
    Ok(_) => {
      let text = line.strip_suffix('\n').unwrap_or(line);
      let text = text.strip_suffix('\r').unwrap_or(text);
      Input::Line(text.to_string())
    }
  }
}

// One terminal event; `None` for the kinds that are not a key press (resize,
// focus, a Windows key release) so the caller reads on.
fn read_key() -> Option<Input> {
  match crossterm::event::read() {
    Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => key_of(key).map(Input::Key),
    Ok(_) => None,
    Err(_) => Some(Input::Eof),
  }
}

fn key_of(key: KeyEvent) -> Option<Key> {
  let mut shift = key.modifiers.contains(KeyModifiers::SHIFT);
  let (name, ch) = match key.code {
    KeyCode::Char(' ') => ("space".to_string(), Some(' ')),
    KeyCode::Char(c) => {
      shift |= c.is_uppercase();
      (c.to_lowercase().to_string(), Some(c))
    }
    KeyCode::Enter => ("return".to_string(), None),
    KeyCode::Backspace => ("backspace".to_string(), None),
    KeyCode::Tab => ("tab".to_string(), None),
    KeyCode::BackTab => {
      shift = true;
      ("tab".to_string(), None)
    }
    KeyCode::Left => ("left".to_string(), None),
    KeyCode::Right => ("right".to_string(), None),
    KeyCode::Up => ("up".to_string(), None),
    KeyCode::Down => ("down".to_string(), None),
    KeyCode::Home => ("home".to_string(), None),
    KeyCode::End => ("end".to_string(), None),
    KeyCode::PageUp => ("pageup".to_string(), None),
    KeyCode::PageDown => ("pagedown".to_string(), None),
    KeyCode::Delete => ("delete".to_string(), None),
    KeyCode::Insert => ("insert".to_string(), None),
    KeyCode::Esc => ("escape".to_string(), None),
    KeyCode::F(n) => (format!("f{n}"), None),
    _ => return None,
  };
  Some(Key {
    name,
    ch,
    ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
    meta: key.modifiers.contains(KeyModifiers::ALT),
    shift,
  })
}

/// Switch the terminal's raw mode: no echo, no line editing, no signal keys
/// (Ctrl-C arrives as a key), and stdin delivers keys instead of lines. `Err`
/// when stdin is not a terminal or the OS refuses. Process-wide state, which
/// the terminal keeps after we exit: see `restore`.
pub fn set_raw_mode(on: bool) -> Result<(), String> {
  if on == RAW.load(Ordering::SeqCst) {
    return Ok(());
  }
  if on {
    if !is_terminal() {
      return Err("stdin is not a terminal this process can use".into());
    }
    terminal::enable_raw_mode().map_err(|e| format!("failed to enable raw mode: {e}"))?;
    // Our own cursor and line sequences are ANSI; the Windows console needs
    // asking (a no-op on Windows Terminal, where it is on already).
    #[cfg(windows)]
    let _ = crossterm::ansi_support::supports_ansi();
  } else {
    terminal::disable_raw_mode().map_err(|e| format!("failed to disable raw mode: {e}"))?;
  }
  RAW.store(on, Ordering::SeqCst);
  Ok(())
}

/// Whether raw mode is on.
pub fn is_raw_mode() -> bool {
  RAW.load(Ordering::SeqCst)
}

/// Leave raw mode if it is on; for the way out (exit, panic hook), where an
/// error has nobody left to report to.
pub fn restore() {
  if RAW.swap(false, Ordering::SeqCst) {
    let _ = terminal::disable_raw_mode();
  }
}

/// Write `text` to stdout as is (no newline appended) and flush, so a prompt
/// shows before the input it waits for.
pub fn write(text: &str) -> std::io::Result<()> {
  let mut out = std::io::stdout().lock();
  out.write_all(text.as_bytes())?;
  out.flush()
}

/// Write `text` as a line. In raw mode the terminal does not return the
/// carriage on "\n" any more, so every line break becomes "\r\n" there; this
/// is what a process logger writes through so its lines stay readable while
/// raw mode is on. Errors are dropped: a logger has nowhere to report them.
pub fn write_line(text: &str) {
  let mut out = std::io::stdout().lock();
  let _ = if RAW.load(Ordering::SeqCst) {
    out.write_all(text.replace('\n', "\r\n").as_bytes()).and_then(|_| out.write_all(b"\r\n"))
  } else {
    out.write_all(text.as_bytes()).and_then(|_| out.write_all(b"\n"))
  };
  let _ = out.flush();
}
