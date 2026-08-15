//! Engine-free subprocess core.
//!
//! The scripting-engine-independent half of `flux:subprocess`: the parsed
//! command spec, buffered `output()` run, and the streaming `spawn()` machinery
//! (the `Child` handle's stdin serialization / kill / status-wait, and the
//! `Supervisor` task that owns the OS child, races a kill request against exit,
//! and publishes the exit status). It names no scripting-engine types; the
//! marshalling layer (`plugins/flux/subprocess.rs`) decodes JS args into a
//! `CommandSpec`, drives these methods, wraps the child's stdout/stderr in the
//! shared body async-iterables, and encodes results back to JS. Destined for the
//! `forge` crate (see REDESIGN.md).
//!
//! Arguments are always an array, never a shell, so there is no per-platform
//! shell/quoting and no injection. `kill_on_drop(true)` reaps a timed-out or
//! abandoned child: dropping the future that owns it (or the `Supervisor`) kills
//! it.

use std::io;
use std::process::Stdio;
use std::rc::Rc;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::{Child as TokioChild, ChildStderr, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::sync::{watch, Mutex, Notify};

use crate::Value;

/// A parsed, reusable command spec. Shared (`Rc`) into each `output()`/`spawn()`
/// so the same reference can be run more than once, like a re-readable `file()`.
pub struct CommandSpec {
  pub cmd: String,
  pub args: Vec<String>,
  pub cwd: Option<String>,
  pub env: Vec<(String, String)>,
  pub stdin: Option<Vec<u8>>,
  pub timeout_ms: Option<u64>,
  pub as_bytes: bool,
}

/// The buffered result of a finished child (`output()`).
pub struct CommandOutput {
  pub code: Option<i32>,
  pub signal: Option<String>,
  pub stdout: Vec<u8>,
  pub stderr: Vec<u8>,
  pub as_bytes: bool,
}

/// The exit status of a spawned child (the `output()` shape without the buffered
/// streams). Cloneable so it can be published to multiple `status()` awaiters.
#[derive(Clone, Default)]
pub struct StatusData {
  pub code: Option<i32>,
  pub signal: Option<String>,
}

/// The `{ code, signal, success }` fields shared by `output()` and `status()`.
fn status_fields(code: Option<i32>, signal: Option<String>) -> Vec<(String, Value)> {
  vec![
    ("code".to_string(), Value::from(code)),
    ("signal".to_string(), Value::from(signal)),
    ("success".to_string(), Value::from(code == Some(0))),
  ]
}

impl From<StatusData> for Value {
  fn from(s: StatusData) -> Value {
    Value::Map(status_fields(s.code, s.signal))
  }
}

/// `{ code, signal, success, stdout, stderr }`; the streams are bytes when the
/// spec asked for `"buffer"` encoding, otherwise lossy UTF-8 strings.
impl From<CommandOutput> for Value {
  fn from(o: CommandOutput) -> Value {
    let stream = |bytes: Vec<u8>| {
      if o.as_bytes {
        Value::bytes(bytes)
      } else {
        Value::String(String::from_utf8_lossy(&bytes).into_owned())
      }
    };
    let mut m = status_fields(o.code, o.signal);
    m.push(("stdout".to_string(), stream(o.stdout)));
    m.push(("stderr".to_string(), stream(o.stderr)));
    Value::Map(m)
  }
}

impl CommandSpec {
  /// Run the child to completion, buffering stdout/stderr. `kill_on_drop` reaps a
  /// timed-out child: on timeout the future owning the child is dropped, killing
  /// it. Failures come back as plain message strings.
  pub async fn run_output(&self) -> Result<CommandOutput, String> {
    let mut command = TokioCommand::new(&self.cmd);
    command.args(&self.args);
    command.kill_on_drop(true);
    command.stdin(if self.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if let Some(cwd) = &self.cwd {
      command.current_dir(cwd);
    }
    for (k, v) in &self.env {
      command.env(k, v);
    }

    let mut child = command.spawn().map_err(|e| spawn_err(&self.cmd, e))?;

    if let Some(bytes) = &self.stdin {
      if let Some(mut si) = child.stdin.take() {
        si.write_all(bytes).await.map_err(|e| format!("failed to write stdin to {}: {e}", self.cmd))?;
        // Dropping si closes stdin so the child sees EOF.
      }
    }

    let output = match self.timeout_ms {
      Some(ms) => match tokio::time::timeout(Duration::from_millis(ms), child.wait_with_output()).await {
        Ok(r) => r.map_err(|e| format!("failed to run {}: {e}", self.cmd))?,
        Err(_) => return Err(format!("command timed out after {ms}ms: {}", self.cmd)),
      },
      None => child.wait_with_output().await.map_err(|e| format!("failed to run {}: {e}", self.cmd))?,
    };

    Ok(CommandOutput {
      code: output.status.code(),
      signal: exit_signal(&output.status),
      stdout: output.stdout,
      stderr: output.stderr,
      as_bytes: self.as_bytes,
    })
  }

  /// Spawn the child with piped stdio and set up its supervisor. Returns the
  /// engine-free `Child` handle, the raw stdout/stderr (the caller wraps them in
  /// its own stream type), the `Supervisor` (the caller spawns `run()`), and any
  /// initial stdin to write (the caller spawns a `write_stdin`). A failure to
  /// launch is a plain message string. Spawning the tasks is host-specific, so it
  /// stays with the caller.
  pub fn spawn(&self) -> Result<Spawned, String> {
    let mut command = TokioCommand::new(&self.cmd);
    command.args(&self.args);
    command.kill_on_drop(true);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    if let Some(cwd) = &self.cwd {
      command.current_dir(cwd);
    }
    for (k, v) in &self.env {
      command.env(k, v);
    }

    let mut child = command.spawn().map_err(|e| spawn_err(&self.cmd, e))?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("child stdout piped");
    let stderr = child.stderr.take().expect("child stderr piped");

    // stdin is behind an async Mutex so write()/endStdin() serialize: a write
    // holds the lock across its (backpressure-respecting) write_all, so concurrent
    // writes queue instead of racing.
    let stdin = Rc::new(Mutex::new(child.stdin.take()));
    let kill_notify = Rc::new(Notify::new());
    let (status_tx, status_rx) = watch::channel(None::<StatusData>);

    let handle = Child { pid, stdin, kill_notify: kill_notify.clone(), status_rx };
    let supervisor = Supervisor { child, kill_notify, status_tx };
    Ok(Spawned { child: handle, stdout, stderr, supervisor, initial_stdin: self.stdin.clone() })
  }
}

/// The pieces of a freshly spawned child the caller wires up: the `Child` handle,
/// its stdout/stderr (to wrap in the host's stream type), the `Supervisor` (to
/// spawn), and any initial stdin (to write on a spawned task).
pub struct Spawned {
  pub child: Child,
  pub stdout: ChildStdout,
  pub stderr: ChildStderr,
  pub supervisor: Supervisor,
  pub initial_stdin: Option<Vec<u8>>,
}

/// A handle to a spawned child: interactive stdin, kill, and exit status. Cloned
/// into each host callback and the initial-stdin task; all clones share the same
/// stdin pipe, kill signal, and status channel.
#[derive(Clone)]
pub struct Child {
  pid: Option<u32>,
  stdin: Rc<Mutex<Option<ChildStdin>>>,
  kill_notify: Rc<Notify>,
  status_rx: watch::Receiver<Option<StatusData>>,
}

impl Child {
  pub fn pid(&self) -> Option<u32> {
    self.pid
  }

  /// Write to the child's stdin, serialized behind the stdin lock. Errors if
  /// stdin has been closed (`end_stdin`) or the write fails.
  pub async fn write_stdin(&self, bytes: Vec<u8>) -> Result<(), String> {
    let mut guard = self.stdin.lock().await;
    match guard.as_mut() {
      Some(si) => si.write_all(&bytes).await.map_err(|e| format!("write stdin: {e}")),
      None => Err("stdin is closed".to_string()),
    }
  }

  /// Close stdin so the child sees EOF, after any queued writes drain (it takes
  /// the same lock).
  pub async fn end_stdin(&self) {
    self.stdin.lock().await.take();
  }

  /// Request termination (portable; SIGKILL / TerminateProcess via the
  /// supervisor's `start_kill`).
  pub fn kill(&self) {
    self.kill_notify.notify_one();
  }

  /// Resolve when the child exits, with its exit status. Multiple callers may
  /// await independently; each sees the same published status.
  pub async fn status(&self) -> StatusData {
    let mut rx = self.status_rx.clone();
    loop {
      if let Some(d) = rx.borrow_and_update().clone() {
        return d;
      }
      if rx.changed().await.is_err() {
        return StatusData::default();
      }
    }
  }
}

/// Owns the OS child for its whole life: waits for exit (or a kill request) and
/// publishes the exit status over a watch channel. The caller spawns `run()` and
/// holds the engine alive for its duration.
pub struct Supervisor {
  child: TokioChild,
  kill_notify: Rc<Notify>,
  status_tx: watch::Sender<Option<StatusData>>,
}

impl Supervisor {
  pub async fn run(mut self) {
    let status = loop {
      tokio::select! {
        _ = self.kill_notify.notified() => { let _ = self.child.start_kill(); }
        res = self.child.wait() => break res,
      }
    };
    let data = match status {
      Ok(st) => StatusData { code: st.code(), signal: exit_signal(&st) },
      Err(_) => StatusData::default(),
    };
    let _ = self.status_tx.send(Some(data));
  }
}

/// `which(cmd)` -> absolute path to the resolved executable, or `None`.
/// Cross-platform PATH lookup (handles Windows PATHEXT / .exe).
pub fn which(cmd: String) -> Option<String> {
  which::which(cmd).ok().map(|p| p.to_string_lossy().into_owned())
}

fn spawn_err(cmd: &str, e: io::Error) -> String {
  if e.kind() == io::ErrorKind::NotFound {
    format!("command not found: {cmd}")
  } else {
    format!("failed to spawn {cmd}: {e}")
  }
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<String> {
  use std::os::unix::process::ExitStatusExt;
  status.signal().map(signal_name)
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<String> {
  None
}

#[cfg(unix)]
fn signal_name(num: i32) -> String {
  match num {
    1 => "SIGHUP".to_string(),
    2 => "SIGINT".to_string(),
    3 => "SIGQUIT".to_string(),
    9 => "SIGKILL".to_string(),
    11 => "SIGSEGV".to_string(),
    13 => "SIGPIPE".to_string(),
    15 => "SIGTERM".to_string(),
    other => format!("SIG{other}"),
  }
}
