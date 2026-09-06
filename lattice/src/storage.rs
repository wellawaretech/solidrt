// Client storage: data-root resolution and the on-disk tree
// (okf/plans/client-storage-updates.md, stage 1 + layout revision 2026-07-21;
// player layout + run marker: okf/backlog/parallel-dev-servers.md stage 3).
//
// Three layouts, picked by how the process was launched. Two independent
// facts distinguish them: is there a client level (only under an explicit
// --data-root, where several numbered clients coexist), and is there an
// apps/ level (everywhere except a packed app, which is exactly one app).
// The fourth combination (one app, many clients) is meaningless.
//
// Dev client (--data-root; srt forwards one for every client it spawns,
// pointing at the dev dotdir's clients/ folder). Many numbered clients,
// many apps:
//
//   <data-root>/client<N>/
//     identity/          client identity (persisted iroh key)
//     apps/<app-id>/
//       data/            the app's mutable sandbox (sqlite, file() writes)
//       cache/           the app's caches (fetch disk cache)
//     logs/
//
// Packed app (app id from the pack manifest): an installed app has exactly
// one client and one app, so neither appears as a directory. The pref path
// under the shared SolidRT vendor level, keyed by the app id, IS the app's
// folder (Flatpak-style grouping: many small apps do not clutter the
// platform data dir, and the machine has one SolidRT folder total):
//
//   <pref SolidRT/app-id>/    e.g. ~/.local/share/SolidRT/com.example.app/
//     identity/  data/  cache/  logs/
//
// Player (neither): one install is one client, so there is no client
// level, but it installs many apps:
//
//   <pref SolidRT/go>/
//     identity/  apps/<app-id>/data/  apps/<app-id>/cache/  logs/
//
// --client <N> selects a tree under an explicit --data-root (default 0; the
// number is chosen by the user, never auto-allocated, so a client's data and
// identity stay put across runs). The other layouts have exactly one client,
// so there it is ignored with a warning. Data always lives on the machine
// the client process runs on; remote dev clients resolve their own local
// root, never the dev server's project dir.
//
// Each client tree also carries run.pid, a marker claiming the tree for one
// live client process (see claim_run_marker).

use std::io::{Read, Write};
use std::path::PathBuf;

/// Startup inputs the resolution runs on: CLI flags plus the packed app id
/// (from the pack manifest). The CLI guarantees a non-empty, path-safe id.
pub struct StorageSpec {
  pub data_root: Option<PathBuf>,
  pub client: Option<u32>,
  pub app_id: Option<String>,
}

/// The resolved per-client directories every storage consumer reads from.
pub struct Storage {
  // The client's root: <data-root or go pref>/client<N>, or the pref path
  // itself for a packed app.
  pub client_dir: PathBuf,
  // The app sandbox the process is anchored in (.../data).
  pub data_dir: PathBuf,
  // Packed flat layout: the root is the single app's dir, no apps/ level.
  pub(crate) flat: bool,
  // The claimed run marker: holding the file keeps its OS lock for the life
  // of the process (a keep-alive, only ever read by tests). None when
  // another live client owns the tree (warned at resolve) or the marker
  // could not be opened.
  #[cfg_attr(not(test), allow(dead_code))]
  pub(crate) run_marker: Option<std::fs::File>,
}

impl Storage {
  /// The app dir for an app named at runtime (a dev push's manifest appId,
  /// unlike the startup id baked into `data_dir`): `apps/<app-id>` under the
  /// client, or the root itself in the packed flat layout (one app per root).
  /// Unsafe ids fall back to "default" like every other component.
  pub fn app_dir(&self, app_id: &str) -> PathBuf {
    if self.flat {
      return self.client_dir.clone();
    }
    self.client_dir.join("apps").join(checked_component(Some(app_id), "app id"))
  }

  /// The app's cache dir (fetch disk cache): `<app_dir>/cache`, so cached
  /// assets are browsable and clearable per app, and die with the app on
  /// remove. Created lazily by the cache on first write.
  pub fn cache_dir(&self, app_id: &str) -> PathBuf {
    self.app_dir(app_id).join("cache")
  }

  /// `<client_dir>/identity` - persisted client identity (p2p key).
  pub fn identity_dir(&self) -> PathBuf {
    self.client_dir.join("identity")
  }

  /// `<client_dir>/apps` - the per-app dirs (installs + data sandboxes). None
  /// in the packed flat layout, which has a single app and no apps/ level.
  pub fn apps_root(&self) -> Option<PathBuf> {
    (!self.flat).then(|| self.client_dir.join("apps"))
  }
}

// A single path component under our control (app id): no separators or
// traversal, so a flag or manifest value cannot escape the tree.
pub(crate) fn safe_component(name: &str) -> bool {
  !name.is_empty()
    && name.len() <= 255
    && name != "."
    && name != ".."
    && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn checked_component(name: Option<&str>, what: &str) -> String {
  match name {
    Some(name) if safe_component(name) => name.to_string(),
    Some(name) => {
      log::warn!("[srt] invalid {what} {name:?}, using \"default\"");
      "default".to_string()
    }
    None => "default".to_string(),
  }
}

/// Resolve the tree for a spec and create its directories. Pure with respect
/// to globals; `init` stores the result for process-wide consumers.
pub(crate) fn resolve(spec: &StorageSpec) -> Option<Storage> {
  let client = || format!("client{}", spec.client.unwrap_or(0));
  let pref = |app: &str| match alloy::sdl3::filesystem::get_pref_path("SolidRT", app) {
    Ok(dir) => Some(dir),
    Err(e) => {
      log::warn!("[srt] no writable pref path: {e}");
      None
    }
  };
  let (client_dir, flat) = match &spec.data_root {
    // Absolutize against the launch cwd: the runtime chdirs into the app
    // sandbox right after resolution, which must not move the root.
    Some(path) => match std::path::absolute(path) {
      Ok(root) => (root.join(client()), false),
      Err(e) => {
        log::warn!("[srt] cannot resolve data root {}: {e}", path.display());
        return None;
      }
    },
    None => {
      // Both pref-path layouts have exactly one client per install, so
      // --client is data-root-only.
      if spec.client.is_some() {
        log::warn!("[srt] --client only applies with --data-root, ignoring");
      }
      match &spec.app_id {
        Some(app_id) => (pref(&checked_component(Some(app_id), "app id"))?, true),
        None => (pref("go")?, false),
      }
    }
  };

  let data_dir = if flat {
    client_dir.join("data")
  } else {
    let app_id = checked_component(spec.app_id.as_deref(), "app id");
    client_dir.join("apps").join(app_id).join("data")
  };
  for dir in [&data_dir, &client_dir.join("identity"), &client_dir.join("logs")] {
    if let Err(e) = std::fs::create_dir_all(dir) {
      log::warn!("[srt] cannot create storage dir {}: {e}", dir.display());
      return None;
    }
  }
  // Dev trees only: the pref-path layouts (packed app, player, Android)
  // have one client per install by construction, so no marker is claimed and
  // nothing dev-shaped is written there.
  let run_marker = if spec.data_root.is_some() { claim_run_marker(&client_dir) } else { None };
  Some(Storage { data_dir, client_dir, flat, run_marker })
}

// Two live clients on one tree share one identity key and one data sandbox
// per app, and corrupt them silently; claim a marker so the second one warns
// loudly instead. Liveness is the OS file lock itself, held for the life of
// the process and released by the OS on any exit including kill -9, so a
// marker is never stale; the pid written inside only names the holder in the
// warning. A warning, not a lock: the client still runs
// (okf/plans/client-storage-updates.md deliberately dropped the
// running-instance lock).
fn claim_run_marker(client_dir: &PathBuf) -> Option<std::fs::File> {
  let path = client_dir.join("run.pid");
  let mut file = match std::fs::OpenOptions::new().read(true).write(true).create(true).open(&path) {
    Ok(file) => file,
    Err(e) => {
      log::warn!("[srt] cannot open run marker {}: {e}", path.display());
      return None;
    }
  };
  match file.try_lock() {
    Ok(()) => {
      let _ = file.set_len(0);
      let _ = file.write_all(std::process::id().to_string().as_bytes());
      Some(file)
    }
    Err(std::fs::TryLockError::WouldBlock) => {
      let mut holder = String::new();
      let _ = file.read_to_string(&mut holder);
      let holder = holder.trim();
      let holder = if holder.is_empty() { String::new() } else { format!(" (pid {holder})") };
      log::warn!(
        "[srt] another client{holder} is already using {}; two clients on one tree corrupt each other's data - give each its own --client number",
        client_dir.display()
      );
      None
    }
    Err(std::fs::TryLockError::Error(e)) => {
      log::warn!("[srt] cannot lock run marker {}: {e}", path.display());
      None
    }
  }
}

static STORAGE: std::sync::OnceLock<Option<Storage>> = std::sync::OnceLock::new();

/// Resolve once at startup and publish for process-wide consumers (cwd
/// anchor, fetch cache, dev-client config). Later calls are ignored.
pub fn init(spec: &StorageSpec) {
  let _ = STORAGE.set(resolve(spec));
}

/// The resolved storage, or None when no writable location exists (the
/// consumers then degrade: unanchored cwd, caching disabled, no persisted
/// config - same behavior as a failed pref path before).
pub fn get() -> Option<&'static Storage> {
  STORAGE.get().and_then(|storage| storage.as_ref())
}
