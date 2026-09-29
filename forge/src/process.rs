//! Engine-free process core: host metadata and OS signal sources.
//!
//! Names no scripting-engine types. The marshalling layer (flux
//! `forge_plugins/process.rs`) owns the event-bus wiring (`ctx.spawn`,
//! emit/has-listeners, the per-context dedup) and forwards to the pieces here:
//! host metadata (`platform`/`arch`/`rss`/`home_dir`/`exec_path`/`env_vars`, the OS and host
//! names), `kill`/`alive`, `exit`, the allocator setting
//! (`return_large_allocations`), and `SignalStream`, which hides the
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

/// The OS process id of the current process.
pub fn pid() -> u32 {
  std::process::id()
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

/// The path of the running executable, or `None` when the OS cannot name it
/// (a deleted or unlinked binary, a platform without the query). What a tool
/// passes to spawn another instance of itself.
pub fn exec_path() -> Option<String> {
  std::env::current_exe().ok().and_then(|path| path.to_str().map(String::from))
}

/// The OS as a person names it: "Linux (Arch Linux)", "Android 15 on Pixel 9
/// Pro" (sysinfo folds the device model in), "macOS 15.2", "Windows 11 Pro".
/// `None` when the platform does not say.
pub fn os_description() -> Option<String> {
  System::long_os_version()
}

/// The kernel version ("7.1.4-arch1-1", a Darwin or NT build), or `None`.
pub fn kernel_version() -> Option<String> {
  System::kernel_version()
}

/// The machine's hostname, or `None`. What tells one client's machine from
/// another's once clients connect over the network.
pub fn host_name() -> Option<String> {
  System::host_name()
}

/// The process environment as name/value pairs (non-UTF-8 entries skipped).
pub fn env_vars() -> Vec<(String, String)> {
  std::env::vars().collect()
}

/// Terminate the process with id `pid`. `false` when no such process exists or
/// the OS refused (permissions). SIGKILL / TerminateProcess, like `Child::kill`.
pub fn kill(pid: u32) -> bool {
  let pid = sysinfo::Pid::from_u32(pid);
  let mut system = System::new_with_specifics(RefreshKind::nothing());
  system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
  system.process(pid).map(|proc| proc.kill()).unwrap_or(false)
}

/// Whether a process with id `pid` exists. A zombie (exited, not yet reaped)
/// counts as gone: it holds no port and serves nothing. What a registry reader
/// asks before trusting a record.
pub fn alive(pid: u32) -> bool {
  let pid = sysinfo::Pid::from_u32(pid);
  let mut system = System::new_with_specifics(RefreshKind::nothing());
  system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
  system.process(pid).is_some_and(|proc| !matches!(proc.status(), sysinfo::ProcessStatus::Zombie))
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

/// End the current process now with `code` as its exit status. Nothing
/// pending runs: what is left is the terminal put back out of raw mode and
/// stdout and stderr flushed, so what was printed is not lost.
pub fn exit(code: u8) -> ! {
  use std::io::Write;
  crate::tty::restore();
  let _ = std::io::stdout().flush();
  let _ = std::io::stderr().flush();
  std::process::exit(i32::from(code))
}

/// The size from which glibc serves an allocation as a mapping of its own,
/// which goes back to the system the moment it is freed: glibc's own
/// default, 128 KiB.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const GLIBC_MMAP_THRESHOLD: libc::c_int = 128 * 1024;

/// Make the process give large buffers back to the system when they are
/// freed. Call once, at startup.
///
/// glibc does that for an allocation above its mmap threshold, but it
/// raises the threshold by itself each time such a block is freed (up to
/// 32 MiB), after which buffers the size of a texture or a decoded image
/// come from the arena heaps, which shrink only from their top: a burst
/// of them at load leaves the process as large as its peak for good.
/// Setting the threshold, even to its default, switches the adjustment
/// off. Measured on a 69 texture model: 110 MB kept without, 15 MB with,
/// and no cost to the frame that this machine could resolve.
///
/// glibc only. The other platforms' allocators do not have the
/// adjustment, and this does nothing there.
pub fn return_large_allocations() {
  #[cfg(all(target_os = "linux", target_env = "gnu"))]
  // SAFETY: mallopt sets a parameter of the allocator; it takes no pointer
  // and may be called at any time. A refusal (0) leaves the default
  // behavior, which is what there was before.
  unsafe {
    libc::mallopt(libc::M_MMAP_THRESHOLD, GLIBC_MMAP_THRESHOLD);
  }
}
