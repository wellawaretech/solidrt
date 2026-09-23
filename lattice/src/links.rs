// Links from outside for a packed desktop app: the scheme it answers to,
// the hand-off from a second instance, and the OS registration. Desktop
// only: on Android the package declares the scheme and the activity is
// singleInstance; on macOS a bundle declares it and LaunchServices keeps one
// instance (not built yet, okf/backlog/deep-links.md).
//
// The scheme is the app id itself, the RFC 8252 section 7.1 private-use
// form (reverse-DNS, `com.example.app://settings`): every packed app answers
// to its own scheme with nothing to declare and nothing to collide with.
//
// Windows and Linux start a new process for a link. The first instance
// listens on a local endpoint under the app's own client dir (a Unix domain
// socket there, a named pipe on Windows, both reachable by this user only);
// a second instance started with a link hands it over there and exits, so
// the app opens once. The protocol is one line each way: the link, then an
// acknowledgement, and the second instance only exits once it has that
// (a first instance on its way out would otherwise swallow the link).
//
// Registration is an explicit call (srt:app registerProtocolHandler, the
// web's name), never an implicit first-run write: a packed app is a single
// executable nothing installs, so the app itself makes this copy the
// handler, and does it again after it moved. Linux writes the freedesktop
// pieces (a .desktop file and the mimeapps.list default), Windows the
// HKCU\Software\Classes protocol key.

use std::path::{Path, PathBuf};

use alloy::{AlloyCommand, AlloyEvent};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

// The longest link the hand-off accepts, bytes: a link is a short string,
// and the cap keeps a misbehaving local peer from holding the reader.
const LINK_MAX_BYTES: u64 = 16 * 1024;
// How long either side of a hand-off waits for the other, ms: both are local
// processes, so anything longer than this is a wedged peer.
const HAND_OFF_TIMEOUT_MS: u64 = 2000;
const ACK: &[u8] = b"ok\n";

/// Whether `arg` is a link of the app's own scheme: `<app_id>:` followed by
/// anything, the scheme compared case-insensitively as schemes are.
pub fn own_link(app_id: &str, arg: &str) -> bool {
  let n = app_id.len();
  arg.len() > n && arg.is_char_boundary(n) && arg[..n].eq_ignore_ascii_case(app_id) && arg.as_bytes()[n] == b':'
}

/// Hand `link` to a running instance of the app, if there is one: true when
/// it acknowledged, and this process should end without opening a window.
/// False when nothing answers (no instance, or a stale endpoint), in which
/// case this process is the instance.
pub fn hand_off(app_id: &str, link: &str) -> bool {
  let Some(client_dir) = crate::storage::pref_dir(app_id) else {
    return false;
  };
  platform::hand_off(app_id, &client_dir, link)
}

/// Deliver one link over an open hand-off connection and wait for the
/// acknowledgement. Sync: the second instance has nothing else to do.
fn deliver<S: std::io::Read + std::io::Write>(stream: &mut S, link: &str) -> bool {
  if stream.write_all(link.as_bytes()).and_then(|_| stream.write_all(b"\n")).is_err() {
    return false;
  }
  let mut ack = vec![0u8; ACK.len()];
  stream.read_exact(&mut ack).is_ok() && ack == ACK
}

/// Answer hand-offs for the life of the process: the first instance's side.
/// Each link received is delivered like an OS-routed one (AlloyEvent::Link)
/// and the window is raised. Silently steps aside when another instance
/// already answers on the endpoint.
pub fn listen(
  app_id: &str,
  client_dir: PathBuf,
  handle: &tokio::runtime::Handle,
  events: tokio::sync::mpsc::UnboundedSender<AlloyEvent>,
  commands: std::sync::mpsc::Sender<AlloyCommand>,
) {
  platform::listen(app_id.to_string(), client_dir, handle, events, commands);
}

/// One accepted hand-off connection: read the link, deliver it, acknowledge.
async fn serve<S>(stream: S, app_id: String, events: tokio::sync::mpsc::UnboundedSender<AlloyEvent>, commands: std::sync::mpsc::Sender<AlloyCommand>)
where
  S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
  let (reader, mut writer) = tokio::io::split(stream);
  let mut line = String::new();
  let mut reader = tokio::io::BufReader::new(reader).take(LINK_MAX_BYTES);
  let read = reader.read_line(&mut line);
  match tokio::time::timeout(std::time::Duration::from_millis(HAND_OFF_TIMEOUT_MS), read).await {
    Ok(Ok(n)) if n > 0 => {}
    Ok(Ok(_)) => return,
    Ok(Err(e)) => {
      log::warn!("[srt] link hand-off read failed: {e}");
      return;
    }
    Err(_) => {
      log::warn!("[srt] link hand-off timed out");
      return;
    }
  }
  let link = line.trim_end_matches(['\n', '\r']).to_string();
  // A local peer can send anything; only the app's own links are links.
  if !own_link(&app_id, &link) {
    log::warn!("[srt] link hand-off ignored: not a {app_id}: link");
    return;
  }
  let _ = events.send(AlloyEvent::Link { link });
  let _ = commands.send(AlloyCommand::RaiseWindow);
  if let Err(e) = writer.write_all(ACK).await {
    log::warn!("[srt] link hand-off acknowledgement failed: {e}");
  }
}

/// Make this executable the handler for the app's scheme on this machine,
/// for this user. `display_name` is what the OS shows for the handler.
pub fn register(app_id: &str, display_name: Option<&str>) -> Result<(), String> {
  let exe = std::env::current_exe().map_err(|e| format!("cannot resolve the executable path: {e}"))?;
  platform::register(app_id, display_name.unwrap_or(app_id), &exe)
}

#[cfg(unix)]
mod platform {
  use super::*;
  use std::os::unix::net::UnixStream;
  use std::time::Duration;

  const ENDPOINT_FILE: &str = "link.sock";

  pub(super) fn hand_off(_app_id: &str, client_dir: &Path, link: &str) -> bool {
    let path = client_dir.join(ENDPOINT_FILE);
    let Ok(mut stream) = UnixStream::connect(&path) else {
      return false;
    };
    let timeout = Some(Duration::from_millis(HAND_OFF_TIMEOUT_MS));
    if stream.set_write_timeout(timeout).is_err() || stream.set_read_timeout(timeout).is_err() {
      return false;
    }
    deliver(&mut stream, link)
  }

  pub(super) fn listen(
    app_id: String,
    client_dir: PathBuf,
    handle: &tokio::runtime::Handle,
    events: tokio::sync::mpsc::UnboundedSender<AlloyEvent>,
    commands: std::sync::mpsc::Sender<AlloyCommand>,
  ) {
    let path = client_dir.join(ENDPOINT_FILE);
    // The socket file outlives an instance that did not exit cleanly. Live
    // or stale is decided the way a second instance decides it: whether
    // anything answers. Nothing does, so the file is ours to replace.
    if UnixStream::connect(&path).is_ok() {
      log::info!("[srt] another instance already answers links at {}", path.display());
      return;
    }
    let _ = std::fs::remove_file(&path);
    handle.spawn(async move {
      let listener = match tokio::net::UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
          log::warn!("[srt] cannot listen for links at {}: {e}", path.display());
          return;
        }
      };
      log::info!("[srt] answering {app_id}: links at {}", path.display());
      loop {
        match listener.accept().await {
          Ok((stream, _)) => {
            tokio::spawn(serve(stream, app_id.clone(), events.clone(), commands.clone()));
          }
          Err(e) => log::warn!("[srt] link hand-off accept failed: {e}"),
        }
      }
    });
  }

  #[cfg(target_os = "macos")]
  pub(super) fn register(_app_id: &str, _display_name: &str, _exe: &Path) -> Result<(), String> {
    Err("on macOS the app bundle declares its scheme (CFBundleURLTypes); nothing is registered at runtime".to_string())
  }

  #[cfg(target_os = "android")]
  pub(super) fn register(_app_id: &str, _display_name: &str, _exe: &Path) -> Result<(), String> {
    // Never reached: lib.rs answers on Android without calling here (the
    // package declares the scheme), and no desktop hand-off runs there.
    Ok(())
  }

  #[cfg(not(any(target_os = "macos", target_os = "android")))]
  pub(super) fn register(app_id: &str, display_name: &str, exe: &Path) -> Result<(), String> {
    freedesktop::register(app_id, display_name, exe)
  }
}

// The freedesktop way (Linux and the BSDs): a desktop entry for this
// executable declaring the scheme's MIME type, and the user's mimeapps.list
// naming it the default handler, exactly what `xdg-mime default` writes.
// Both written directly rather than through xdg-utils, so registration
// does not depend on tools on PATH and behaves the same on every desktop.
// GIO and xdg-open read mimeapps.list for the default and find the entry by
// its id under the applications dirs; no cache rebuild is involved.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android"))))]
pub(crate) mod freedesktop {
  use super::*;

  const DEFAULTS_SECTION: &str = "[Default Applications]";
  const ADDED_SECTION: &str = "[Added Associations]";

  pub(super) fn register(app_id: &str, display_name: &str, exe: &Path) -> Result<(), String> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("HOME is not set")?;
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    let desktop_id = format!("{app_id}.desktop");

    let applications = data_home.join("applications");
    std::fs::create_dir_all(&applications).map_err(|e| format!("cannot create {}: {e}", applications.display()))?;
    write_atomically(&applications.join(&desktop_id), desktop_entry(app_id, display_name, exe).as_bytes())?;

    std::fs::create_dir_all(&config_home).map_err(|e| format!("cannot create {}: {e}", config_home.display()))?;
    let mimeapps = config_home.join("mimeapps.list");
    let existing = match std::fs::read_to_string(&mimeapps) {
      Ok(text) => text,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
      Err(e) => return Err(format!("cannot read {}: {e}", mimeapps.display())),
    };
    let updated = with_default(&existing, &scheme_mime(app_id), &desktop_id);
    write_atomically(&mimeapps, updated.as_bytes())
  }

  pub(crate) fn scheme_mime(app_id: &str) -> String {
    format!("x-scheme-handler/{app_id}")
  }

  /// The desktop entry: `%u` receives the link, so the executable gets it as
  /// its first argument (main.rs recognizes the app's own scheme there).
  pub(crate) fn desktop_entry(app_id: &str, display_name: &str, exe: &Path) -> String {
    let name = display_name.replace(['\n', '\r'], " ");
    format!(
      "[Desktop Entry]\nType=Application\nName={name}\nExec={} %u\nTerminal=false\nMimeType={};\n",
      exec_quote(&exe.to_string_lossy()),
      scheme_mime(app_id)
    )
  }

  // The characters the Desktop Entry specification reserves in an Exec
  // argument: an argument holding any of them must be quoted.
  const EXEC_RESERVED: &[char] =
    &[' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')', '`', '%'];

  /// One Exec argument per the Desktop Entry specification: as is when it
  /// holds no reserved character, since some launchers (xdg-open's generic
  /// path among them) take an unquoted first word literally; otherwise
  /// double-quoted, with the four characters the quoting reserves
  /// backslash-escaped, and `%` doubled since it introduces field codes.
  pub(crate) fn exec_quote(arg: &str) -> String {
    if !arg.contains(EXEC_RESERVED) {
      return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    for c in arg.chars() {
      match c {
        '"' | '`' | '$' | '\\' => {
          out.push('\\');
          out.push(c);
        }
        '%' => out.push_str("%%"),
        _ => out.push(c),
      }
    }
    out.push('"');
    out
  }

  /// mimeapps.list with `desktop_id` as the default for `mime` and listed
  /// among its added associations, the two entries `xdg-mime default`
  /// writes. Every other line is kept as it was.
  pub(crate) fn with_default(existing: &str, mime: &str, desktop_id: &str) -> String {
    let mut text = set_key(existing, DEFAULTS_SECTION, mime, desktop_id, false);
    text = set_key(&text, ADDED_SECTION, mime, desktop_id, true);
    text
  }

  // Set `key` in `section` to `value`; `accumulate` keeps an existing
  // semicolon list and adds `value` to it when missing (the associations
  // form), else the value replaces. A missing section is appended.
  fn set_key(existing: &str, section: &str, key: &str, value: &str, accumulate: bool) -> String {
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    let start = match lines.iter().position(|l| l.trim() == section) {
      Some(i) => i,
      None => {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
          lines.push(String::new());
        }
        lines.push(section.to_string());
        lines.len() - 1
      }
    };
    let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |n| start + 1 + n);
    let prefix = format!("{key}=");
    let new_line = |current: Option<&str>| {
      if accumulate {
        let mut ids: Vec<&str> = current.map(|c| c.split(';').filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        if !ids.contains(&value) {
          ids.push(value);
        }
        format!("{prefix}{};", ids.join(";"))
      } else {
        format!("{prefix}{value}")
      }
    };
    match lines[start + 1..end].iter().position(|l| l.starts_with(&prefix)) {
      Some(n) => {
        let i = start + 1 + n;
        lines[i] = new_line(Some(&lines[i][prefix.len()..]));
      }
      None => lines.insert(end, new_line(None)),
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
  }

  // A config file is replaced whole or not at all: written next to its
  // place and renamed over it.
  fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
  }
}

#[cfg(windows)]
mod platform {
  use super::*;
  use std::hash::{Hash, Hasher};

  // Pipe names are machine-global; the client dir is this user's, so its
  // path scopes the name to the user (the app id is in it for readability).
  fn endpoint(app_id: &str, client_dir: &Path) -> String {
    let mut hasher = std::hash::DefaultHasher::new();
    client_dir.hash(&mut hasher);
    format!(r"\\.\pipe\SolidRT.{app_id}.{:016x}", hasher.finish())
  }

  pub(super) fn hand_off(app_id: &str, client_dir: &Path, link: &str) -> bool {
    let Ok(mut pipe) = std::fs::OpenOptions::new().read(true).write(true).open(endpoint(app_id, client_dir)) else {
      return false;
    };
    deliver(&mut pipe, link)
  }

  pub(super) fn listen(
    app_id: String,
    client_dir: PathBuf,
    handle: &tokio::runtime::Handle,
    events: tokio::sync::mpsc::UnboundedSender<AlloyEvent>,
    commands: std::sync::mpsc::Sender<AlloyCommand>,
  ) {
    use tokio::net::windows::named_pipe::ServerOptions;
    let name = endpoint(&app_id, &client_dir);
    handle.spawn(async move {
      // Claiming the first instance fails when the pipe exists: another
      // instance answers, this one stays quiet.
      let mut server = match ServerOptions::new().first_pipe_instance(true).create(&name) {
        Ok(server) => server,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
          log::info!("[srt] another instance already answers links at {name}");
          return;
        }
        Err(e) => {
          log::warn!("[srt] cannot listen for links at {name}: {e}");
          return;
        }
      };
      log::info!("[srt] answering {app_id}: links at {name}");
      loop {
        if let Err(e) = server.connect().await {
          log::warn!("[srt] link hand-off accept failed: {e}");
          continue;
        }
        let connected = server;
        server = match ServerOptions::new().create(&name) {
          Ok(server) => server,
          Err(e) => {
            log::warn!("[srt] cannot re-open the link endpoint {name}: {e}");
            tokio::spawn(serve(connected, app_id.clone(), events.clone(), commands.clone()));
            return;
          }
        };
        tokio::spawn(serve(connected, app_id.clone(), events.clone(), commands.clone()));
      }
    });
  }

  // The per-user protocol key the shell consults for `<scheme>:` links:
  // HKCU\Software\Classes\<scheme> marked as a URL protocol, its open
  // command this executable with the link as the one argument.
  pub(super) fn register(app_id: &str, display_name: &str, exe: &Path) -> Result<(), String> {
    use windows_registry::CURRENT_USER;
    let exe = exe.to_string_lossy();
    let key = CURRENT_USER
      .create(format!(r"Software\Classes\{app_id}"))
      .map_err(|e| format!("cannot create the protocol key: {e}"))?;
    key.set_string("", format!("URL:{display_name}")).map_err(|e| format!("cannot name the protocol: {e}"))?;
    key.set_string("URL Protocol", "").map_err(|e| format!("cannot mark the protocol: {e}"))?;
    key
      .create("DefaultIcon")
      .and_then(|k| k.set_string("", format!("\"{exe}\",0")))
      .map_err(|e| format!("cannot set the icon: {e}"))?;
    key
      .create(r"shell\open\command")
      .and_then(|k| k.set_string("", format!("\"{exe}\" \"%1\"")))
      .map_err(|e| format!("cannot set the open command: {e}"))
  }
}
