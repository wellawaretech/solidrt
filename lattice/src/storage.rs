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
// Dev client (--data-root; sol forwards one for every client it spawns,
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
// Under a dev tree each app dir also carries run.pid, a marker claiming the
// app for one live client process (see Storage::claim_app).

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
  // A dev tree (--data-root), where numbered clients share one root and
  // apps are claimed (see claim_app); the pref-path layouts have one client
  // per install by construction and claim nothing.
  pub(crate) dev: bool,
  // The app this process holds, with its locked run marker: holding the
  // file keeps its OS lock until the claim moves or the process exits.
  pub(crate) claim: std::sync::Mutex<Option<(String, std::fs::File)>>,
}

/// The cache subdirectory of an app dir (see `Storage::cache_dir`).
pub(crate) const CACHE_DIR: &str = "cache";

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

  /// The app's cache dir (fetch disk cache, compiled wasm): `<app_dir>/cache`,
  /// so cached assets are browsable and clearable per app, and die with the
  /// app on remove. Created lazily by a cache on first write.
  pub fn cache_dir(&self, app_id: &str) -> PathBuf {
    self.app_dir(app_id).join(CACHE_DIR)
  }

  /// Claim `app_id` for this process on a dev tree: a second live client of
  /// the same app on the same tree is refused, since the two would corrupt
  /// its installed versions and its data. Clients of different apps share a
  /// tree freely (two projects on client 0). One claim at a time: the new
  /// app is locked before the old one is let go, and reclaiming the held app
  /// is a no-op. The player's own "default" home is never claimed, so idle
  /// players coexist: claiming it lets go of the held app. Liveness is the OS file lock itself, released on any
  /// exit including kill -9, so a marker is never stale; the pid written
  /// inside only names the holder in the refusal.
  pub fn claim_app(&self, app_id: &str) -> Result<(), String> {
    let id = checked_component(Some(app_id), "app id");
    if !self.dev {
      return Ok(());
    }
    let mut held = self.claim.lock().expect("app claim lock poisoned");
    if id == "default" {
      *held = None;
      return Ok(());
    }
    if held.as_ref().is_some_and(|(held_id, _)| *held_id == id) {
      return Ok(());
    }
    let dir = self.app_dir(&id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
      log::warn!("[sol] cannot create app dir {}: {e}", dir.display());
      return Ok(());
    }
    let path = dir.join("run.pid");
    let mut file = match std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path) {
      Ok(file) => file,
      Err(e) => {
        log::warn!("[sol] cannot open run marker {}: {e}", path.display());
        return Ok(());
      }
    };
    match file.try_lock() {
      Ok(()) => {
        let _ = file.set_len(0);
        let _ = file.write_all(std::process::id().to_string().as_bytes());
        *held = Some((id, file));
        Ok(())
      }
      Err(std::fs::TryLockError::WouldBlock) => {
        let mut holder = String::new();
        let _ = file.read_to_string(&mut holder);
        let holder = holder.trim();
        let holder = if holder.is_empty() { String::new() } else { format!(" (pid {holder})") };
        Err(format!(
          "Another client{holder} is running {id} in {}; two clients of one app on one data tree corrupt its versions and data, so give this one its own --client number",
          self.client_dir.display()
        ))
      }
      Err(std::fs::TryLockError::Error(e)) => {
        log::warn!("[sol] cannot lock run marker {}: {e}", path.display());
        Ok(())
      }
    }
  }

  /// `<client_dir>/identity` - persisted client identity (p2p key).
  #[cfg_attr(not(feature = "go"), allow(dead_code))]
  pub fn identity_dir(&self) -> PathBuf {
    self.client_dir.join("identity")
  }

  /// `<client_dir>/apps` - the per-app dirs (installs + data sandboxes). None
  /// in the packed flat layout, which has a single app and no apps/ level.
  #[cfg_attr(not(feature = "go"), allow(dead_code))]
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
      log::warn!("[sol] invalid {what} {name:?}, using \"default\"");
      "default".to_string()
    }
    None => "default".to_string(),
  }
}

/// Resolve the tree for a spec and create its directories. Pure with respect
/// to globals; `init` stores the result for process-wide consumers.
/// The OS-designated per-user directory for one install: `<pref root>/SolidRT/<app>`,
/// created by SDL if missing. The packed app's client root (app = its id),
/// the player's (app = "go"). None without a writable location.
pub(crate) fn pref_dir(app: &str) -> Option<PathBuf> {
  match ore::folders::pref_dir("SolidRT", app) {
    Ok(dir) => Some(dir),
    Err(e) => {
      log::warn!("[sol] no writable pref path: {e}");
      None
    }
  }
}

pub(crate) fn resolve(spec: &StorageSpec) -> Option<Storage> {
  let client = || format!("client{}", spec.client.unwrap_or(0));
  let pref = pref_dir;
  let (client_dir, flat) = match &spec.data_root {
    // Absolutize against the launch cwd: the runtime chdirs into the app
    // sandbox right after resolution, which must not move the root.
    Some(path) => match std::path::absolute(path) {
      Ok(root) => (root.join(client()), false),
      Err(e) => {
        log::warn!("[sol] cannot resolve data root {}: {e}", path.display());
        return None;
      }
    },
    None => {
      // Both pref-path layouts have exactly one client per install, so
      // --client is data-root-only.
      if spec.client.is_some() {
        log::warn!("[sol] --client only applies with --data-root, ignoring");
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
      log::warn!("[sol] cannot create storage dir {}: {e}", dir.display());
      return None;
    }
  }
  let dev = spec.data_root.is_some();
  Some(Storage { data_dir, client_dir, flat, dev, claim: std::sync::Mutex::new(None) })
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

/// `Storage::claim_app` on the process's storage; nothing to claim without one.
pub fn claim_app(app_id: &str) -> Result<(), String> {
  get().map_or(Ok(()), |store| store.claim_app(app_id))
}
