// Client storage: data-root resolution and the on-disk tree
// (okf/plans/client-storage-updates.md, stage 1 + layout revision 2026-07-21).
//
// Three layouts, picked by how the process was launched:
//
// Explicit --data-root (dev; the CLI passes the project-local .srt-data so
// dev state stays with the project). Multiple named clients, multiple apps:
//
//   <data-root>/clients/<name>/
//     identity/          client identity (persisted iroh key)
//     apps/<app-id>/
//       data/            the app's mutable sandbox (sqlite, file() writes)
//     cache/             client-level caches (fetch disk cache)
//     logs/
//
// Packed app (app id from the pack manifest): an installed app has exactly
// one client and one app, so neither appears as a directory. The pref path
// keyed by the app id alone IS the app's folder (empty org skips the vendor
// level on every platform):
//
//   <pref "" app-id>/    e.g. ~/.local/share/com.example.app/
//     identity/  data/  cache/  logs/
//
// Generic go client (neither): one client per device, many apps:
//
//   <pref SolidRT/go>/
//     identity/  apps/<app-id>/data/  cache/  logs/
//
// --client selects a tree only under an explicit --data-root; elsewhere it is
// ignored with a warning. Data always lives on the machine the client process
// runs on; remote dev clients resolve their own local root, never the dev
// server's project dir.

use std::path::PathBuf;

/// Startup inputs the resolution runs on: CLI flags plus the packed app id
/// (from the pack manifest). The CLI guarantees a non-empty, path-safe id.
pub struct StorageSpec {
  pub data_root: Option<PathBuf>,
  pub client: Option<String>,
  pub app_id: Option<String>,
}

/// The resolved per-client directories every storage consumer reads from.
pub struct Storage {
  // The client's root: <data-root>/clients/<name>, or the pref path itself.
  pub client_dir: PathBuf,
  // The app sandbox the process is anchored in (.../data).
  pub data_dir: PathBuf,
  // <client_dir>/cache
  pub cache_dir: PathBuf,
  // Packed flat layout: the root is the single app's dir, no apps/ level.
  pub(crate) flat: bool,
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

  /// `<client_dir>/identity` - persisted client identity (p2p key).
  pub fn identity_dir(&self) -> PathBuf {
    self.client_dir.join("identity")
  }
}

// A single path component under our control (client name, app id): no
// separators or traversal, so a flag or manifest value cannot escape the tree.
fn safe_component(name: &str) -> bool {
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
  if spec.client.is_some() && spec.data_root.is_none() {
    log::warn!("[srt] --client only applies with --data-root, ignoring");
  }
  let (client_dir, flat) = match &spec.data_root {
    // Absolutize against the launch cwd: the runtime chdirs into the app
    // sandbox right after resolution, which must not move the root.
    Some(path) => match std::path::absolute(path) {
      Ok(root) => (root.join("clients").join(checked_component(spec.client.as_deref(), "client name")), false),
      Err(e) => {
        log::warn!("[srt] cannot resolve data root {}: {e}", path.display());
        return None;
      }
    },
    None => {
      let (org, app, flat) = match &spec.app_id {
        Some(app_id) => ("", checked_component(Some(app_id), "app id"), true),
        None => ("SolidRT", "go".to_string(), false),
      };
      match alloy::sdl3::filesystem::get_pref_path(org, &app) {
        Ok(dir) => (dir, flat),
        Err(e) => {
          log::warn!("[srt] no writable pref path: {e}");
          return None;
        }
      }
    }
  };

  let data_dir = if flat {
    client_dir.join("data")
  } else {
    let app_id = checked_component(spec.app_id.as_deref(), "app id");
    client_dir.join("apps").join(app_id).join("data")
  };
  let storage = Storage { data_dir, cache_dir: client_dir.join("cache"), client_dir, flat };
  for dir in
    [&storage.data_dir, &storage.cache_dir, &storage.client_dir.join("identity"), &storage.client_dir.join("logs")]
  {
    if let Err(e) = std::fs::create_dir_all(dir) {
      log::warn!("[srt] cannot create storage dir {}: {e}", dir.display());
      return None;
    }
  }
  Some(storage)
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
