//! One instance per app for links on desktop: Windows and Linux start a new
//! process for a link of the app's own scheme. The first instance listens
//! on a local endpoint under the app's own dir (a Unix domain socket there,
//! a named pipe on Windows, both reachable by this user only); a second
//! instance started with a link hands it over there and exits, so the app
//! opens once. The protocol is one line each way: the link, then an
//! acknowledgement, and the second instance only exits once it has that (a
//! first instance on its way out would otherwise swallow the link).
//!
//! Android's activity is singleInstance and macOS's LaunchServices keeps
//! one instance, so neither calls here (okf/backlog/deep-links.md).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::own_link;

// The longest link the hand-off accepts, bytes: a link is a short string,
// and the cap keeps a misbehaving local peer from holding the reader.
const LINK_MAX_BYTES: u64 = 16 * 1024;
// How long either side of a hand-off waits for the other, ms: both are local
// processes, so anything longer than this is a wedged peer.
const HAND_OFF_TIMEOUT_MS: u64 = 2000;
const ACK: &[u8] = b"ok\n";

/// What a first instance does with each link handed to it.
type OnLink = Arc<dyn Fn(String) + Send + Sync>;

/// Hand `link` to a running instance of the app whose scheme is `scheme`
/// and whose own dir is `dir`, if there is one: true when it acknowledged,
/// and this process should end without opening a window. False when
/// nothing answers (no instance, or a stale endpoint), in which case this
/// process is the instance.
pub fn hand_off(scheme: &str, dir: &Path, link: &str) -> bool {
  platform::hand_off(scheme, dir, link)
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

/// Answer hand-offs for the life of the process, on `handle`: the first
/// instance's side. Each link of the app's own scheme received goes to
/// `on_link`, before the second instance gets its acknowledgement. Silently
/// steps aside when another instance already answers on the endpoint.
pub fn listen<F: Fn(String) + Send + Sync + 'static>(
  scheme: &str,
  dir: PathBuf,
  handle: &tokio::runtime::Handle,
  on_link: F,
) {
  platform::listen(scheme.to_string(), dir, handle, Arc::new(on_link));
}

/// One accepted hand-off connection: read the link, deliver it, acknowledge.
async fn serve<S>(stream: S, scheme: String, on_link: OnLink)
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
      log::warn!("[ore] Link hand-off read failed: {e}");
      return;
    }
    Err(_) => {
      log::warn!("[ore] Link hand-off timed out");
      return;
    }
  }
  let link = line.trim_end_matches(['\n', '\r']).to_string();
  // A local peer can send anything; only the app's own links are links.
  if !own_link(&scheme, &link) {
    log::warn!("[ore] Link hand-off ignored: not a {scheme}: link");
    return;
  }
  on_link(link);
  if let Err(e) = writer.write_all(ACK).await {
    log::warn!("[ore] Link hand-off acknowledgement failed: {e}");
  }
}

#[cfg(unix)]
mod platform {
  use super::*;
  use std::os::unix::net::UnixStream;
  use std::time::Duration;

  const ENDPOINT_FILE: &str = "link.sock";

  pub(super) fn hand_off(_scheme: &str, dir: &Path, link: &str) -> bool {
    let path = dir.join(ENDPOINT_FILE);
    let Ok(mut stream) = UnixStream::connect(&path) else {
      return false;
    };
    let timeout = Some(Duration::from_millis(HAND_OFF_TIMEOUT_MS));
    if stream.set_write_timeout(timeout).is_err() || stream.set_read_timeout(timeout).is_err() {
      return false;
    }
    deliver(&mut stream, link)
  }

  pub(super) fn listen(scheme: String, dir: PathBuf, handle: &tokio::runtime::Handle, on_link: OnLink) {
    let path = dir.join(ENDPOINT_FILE);
    // The socket file outlives an instance that did not exit cleanly. Live
    // or stale is decided the way a second instance decides it: whether
    // anything answers. Nothing does, so the file is ours to replace.
    if UnixStream::connect(&path).is_ok() {
      log::info!("[ore] Another instance already answers links at {}", path.display());
      return;
    }
    let _ = std::fs::remove_file(&path);
    handle.spawn(async move {
      let listener = match tokio::net::UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(e) => {
          log::warn!("[ore] Cannot listen for links at {}: {e}", path.display());
          return;
        }
      };
      log::info!("[ore] Answering {scheme}: links at {}", path.display());
      loop {
        match listener.accept().await {
          Ok((stream, _)) => {
            tokio::spawn(serve(stream, scheme.clone(), on_link.clone()));
          }
          Err(e) => log::warn!("[ore] Link hand-off accept failed: {e}"),
        }
      }
    });
  }
}

#[cfg(windows)]
mod platform {
  use super::*;
  use std::hash::{Hash, Hasher};

  // Pipe names are machine-global; the app's dir is this user's, so its
  // path scopes the name to the user (the scheme is in it for readability).
  fn endpoint(scheme: &str, dir: &Path) -> String {
    let mut hasher = std::hash::DefaultHasher::new();
    dir.hash(&mut hasher);
    format!(r"\\.\pipe\SolidRT.{scheme}.{:016x}", hasher.finish())
  }

  pub(super) fn hand_off(scheme: &str, dir: &Path, link: &str) -> bool {
    let Ok(mut pipe) = std::fs::OpenOptions::new().read(true).write(true).open(endpoint(scheme, dir)) else {
      return false;
    };
    deliver(&mut pipe, link)
  }

  pub(super) fn listen(scheme: String, dir: PathBuf, handle: &tokio::runtime::Handle, on_link: OnLink) {
    use tokio::net::windows::named_pipe::ServerOptions;
    let name = endpoint(&scheme, &dir);
    handle.spawn(async move {
      // Claiming the first instance fails when the pipe exists: another
      // instance answers, this one stays quiet.
      let mut server = match ServerOptions::new().first_pipe_instance(true).create(&name) {
        Ok(server) => server,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
          log::info!("[ore] Another instance already answers links at {name}");
          return;
        }
        Err(e) => {
          log::warn!("[ore] Cannot listen for links at {name}: {e}");
          return;
        }
      };
      log::info!("[ore] Answering {scheme}: links at {name}");
      loop {
        if let Err(e) = server.connect().await {
          log::warn!("[ore] Link hand-off accept failed: {e}");
          continue;
        }
        let connected = server;
        server = match ServerOptions::new().create(&name) {
          Ok(server) => server,
          Err(e) => {
            log::warn!("[ore] Cannot re-open the link endpoint {name}: {e}");
            tokio::spawn(serve(connected, scheme.clone(), on_link.clone()));
            return;
          }
        };
        tokio::spawn(serve(connected, scheme.clone(), on_link.clone()));
      }
    });
  }
}
