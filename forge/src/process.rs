//! Engine-free process core: host metadata and OS signal sources.
//!
//! Names no scripting-engine types. The marshalling layer
//! (`plugins/flux/process.rs`) owns the event-bus wiring (`ctx.spawn`,
//! emit/has-listeners, the per-context dedup) and forwards to the pieces here:
//! host metadata (`platform`/`arch`/`rss`/`home_dir`), `kill`, and `SignalStream`, which hides the
//! unix vs non-unix OS signal split behind one async source.

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// The host OS ("darwin", "win32", "linux", "android", ...).
pub fn platform() -> &'static str {
  match std::env::consts::OS {
    "macos" => "darwin",
    "windows" => "win32",
    other => other,
  }
}

/// The CPU architecture ("x64", "arm64", ...).
pub fn arch() -> &'static str {
  match std::env::consts::ARCH {
    "x86_64" => "x64",
    "aarch64" => "arm64",
    "x86" => "ia32",
    other => other,
  }
}

/// Resident set size of the current process in bytes (0 if unavailable).
pub fn rss() -> u64 {
  let mut system = System::new_with_specifics(RefreshKind::nothing());
  let Ok(pid) = sysinfo::get_current_pid() else {
    return 0;
  };
  system.refresh_processes_specifics(
    ProcessesToUpdate::Some(&[pid]),
    true,
    ProcessRefreshKind::nothing().with_memory(),
  );
  system.process(pid).map(|proc| proc.memory()).unwrap_or(0)
}

/// The current user's home directory, or `None` when the environment does not
/// name one (HOME on unix, USERPROFILE on Windows).
pub fn home_dir() -> Option<String> {
  let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
  std::env::var(var).ok().filter(|v| !v.is_empty())
}

/// Terminate the process with id `pid`. `false` when no such process exists or
/// the OS refused (permissions). SIGKILL / TerminateProcess, like `Child::kill`.
pub fn kill(pid: u32) -> bool {
  let pid = sysinfo::Pid::from_u32(pid);
  let mut system = System::new_with_specifics(RefreshKind::nothing());
  system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
  system.process(pid).map(|proc| proc.kill()).unwrap_or(false)
}

/// Signal names with an OS watcher. Unknown names install no watcher (their
/// listeners simply never fire).
pub const KNOWN_SIGNALS: &[&str] = &["SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT", "SIGUSR1", "SIGUSR2"];

/// A uniform async source of OS signal deliveries, hiding the unix
/// (`tokio::signal::unix`, all of `KNOWN_SIGNALS`) vs non-unix (`ctrl_c`, SIGINT
/// only) split. `open` installs the handler; each `recv().await` resolves on the
/// next delivery and returns `false` once the source ends.
pub struct SignalStream {
  #[cfg(unix)]
  stream: tokio::signal::unix::Signal,
}

impl SignalStream {
  /// Install an OS watcher for `signal`. `Err` if the name is unknown,
  /// unsupported on this platform, or the OS handler could not be installed.
  #[cfg(unix)]
  pub fn open(signal: &str) -> Result<SignalStream, String> {
    use tokio::signal::unix::SignalKind;

    if !KNOWN_SIGNALS.contains(&signal) {
      return Err(format!("unrecognized signal: {signal}"));
    }
    let kind = match signal {
      "SIGINT" => SignalKind::interrupt(),
      "SIGTERM" => SignalKind::terminate(),
      "SIGHUP" => SignalKind::hangup(),
      "SIGQUIT" => SignalKind::quit(),
      "SIGUSR1" => SignalKind::user_defined1(),
      "SIGUSR2" => SignalKind::user_defined2(),
      _ => unreachable!("gated by KNOWN_SIGNALS"),
    };
    let stream = tokio::signal::unix::signal(kind).map_err(|e| format!("failed to install {signal} handler: {e}"))?;
    Ok(SignalStream { stream })
  }

  /// Non-unix: only SIGINT (Ctrl+C) is supported, via tokio's `ctrl_c()`. There
  /// is no persistent stream to hold; each `recv` awaits a fresh `ctrl_c()`.
  #[cfg(not(unix))]
  pub fn open(signal: &str) -> Result<SignalStream, String> {
    if !KNOWN_SIGNALS.contains(&signal) {
      return Err(format!("unrecognized signal: {signal}"));
    }
    if signal != "SIGINT" {
      return Err(format!("unsupported signal on this platform: {signal}"));
    }
    Ok(SignalStream {})
  }

  /// Await the next delivery. Returns `false` when the source ends (the stream
  /// closed, or a non-unix `ctrl_c()` errored).
  #[cfg(unix)]
  pub async fn recv(&mut self) -> bool {
    self.stream.recv().await.is_some()
  }

  #[cfg(not(unix))]
  pub async fn recv(&mut self) -> bool {
    tokio::signal::ctrl_c().await.is_ok()
  }
}
