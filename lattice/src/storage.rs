// Client storage: data-root resolution and the on-disk tree
// (okf/plans/client-storage-updates.md, stage 1).
//
// A client on a machine is a data directory. Under a data root:
//
//   clients/<name>/
//     identity/          client identity (stage 2: persisted iroh key)
//     apps/<app-id>/
//       data/            the app's mutable sandbox (sqlite, file() writes)
//     cache/             client-level caches (fetch disk cache)
//     logs/
//
// The data root is resolved once at startup:
//   1. an explicit --data-root (the dev CLI passes the project-local
//      .srt-data so dev state stays with the project),
//   2. a packed app's own pref path, org/display name from the app-identity
//      trailer section (a packed app never shares the generic client's store),
//   3. the generic client's pref path, SolidRT/go.
// Data always lives on the machine the client process runs on; remote dev
// clients resolve their own local root, never the dev server's project dir.

use std::path::PathBuf;

/// Who a packed app is, from the `srt pack` app-identity trailer section.
/// The CLI guarantees non-empty fields; `org`/`display_name` become the SDL
/// pref-path components, `app_id` the store directory name.
pub struct AppIdentity {
  pub app_id: String,
  pub org: String,
  pub display_name: String,
}

/// Decode the app-identity trailer section: three length-prefixed UTF-8
/// strings [len u8][bytes] (appId, org, displayName), consumed exactly.
/// Not JSON: the encoding predates serde_json in packed builds (adopted for
/// manifest.json in stage 3b) and stays as-is - CLI and runner ship pinned.
pub fn decode_app_identity(bytes: &[u8]) -> Option<AppIdentity> {
  fn take<'a>(cursor: &mut &'a [u8]) -> Option<&'a str> {
    let (&len, rest) = cursor.split_first()?;
    if rest.len() < len as usize {
      return None;
    }
    let (string, rest) = rest.split_at(len as usize);
    *cursor = rest;
    std::str::from_utf8(string).ok()
  }
  let mut cursor = bytes;
  let app_id = take(&mut cursor)?.to_string();
  let org = take(&mut cursor)?.to_string();
  let display_name = take(&mut cursor)?.to_string();
  if !cursor.is_empty() || app_id.is_empty() || org.is_empty() || display_name.is_empty() {
    return None;
  }
  Some(AppIdentity { app_id, org, display_name })
}

/// Startup inputs the resolution runs on: CLI flags plus the packed identity.
pub struct StorageSpec {
  pub data_root: Option<PathBuf>,
  pub client: Option<String>,
  pub identity: Option<AppIdentity>,
}

/// The resolved per-client directories every storage consumer reads from.
pub struct Storage {
  // <root>/clients/<name>
  pub client_dir: PathBuf,
  // <root>/clients/<name>/apps/<app-id>/data - the process is anchored here
  pub data_dir: PathBuf,
  // <root>/clients/<name>/cache
  pub cache_dir: PathBuf,
}

impl Storage {
  /// `clients/<name>/apps/<app-id>` for an app named at runtime (a dev push's
  /// manifest appId, unlike the startup identity baked into `data_dir`).
  /// Unsafe ids fall back to "default" like every other component.
  pub fn app_dir(&self, app_id: &str) -> PathBuf {
    self.client_dir.join("apps").join(checked_component(Some(app_id), "app id"))
  }

  /// `clients/<name>/identity` - persisted client identity (p2p key).
  pub fn identity_dir(&self) -> PathBuf {
    self.client_dir.join("identity")
  }
}

// A single path component under our control (client name, app id): no
// separators or traversal, so a flag or trailer value cannot escape the tree.
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
  let root = match &spec.data_root {
    // Absolutize against the launch cwd: the runtime chdirs into the app
    // sandbox right after resolution, which must not move the root.
    Some(path) => match std::path::absolute(path) {
      Ok(path) => path,
      Err(e) => {
        log::warn!("[srt] cannot resolve data root {}: {e}", path.display());
        return None;
      }
    },
    None => {
      let (org, app) = match &spec.identity {
        Some(id) => (id.org.as_str(), id.display_name.as_str()),
        None => ("SolidRT", "go"),
      };
      match alloy::sdl3::filesystem::get_pref_path(org, app) {
        Ok(dir) => dir,
        Err(e) => {
          log::warn!("[srt] no writable pref path: {e}");
          return None;
        }
      }
    }
  };

  let client = checked_component(spec.client.as_deref(), "client name");
  let app_id = checked_component(spec.identity.as_ref().map(|id| id.app_id.as_str()), "app id");

  let client_dir = root.join("clients").join(client);
  let storage = Storage {
    data_dir: client_dir.join("apps").join(app_id).join("data"),
    cache_dir: client_dir.join("cache"),
    client_dir,
  };
  for dir in [&storage.data_dir, &storage.cache_dir, &storage.client_dir.join("identity"), &storage.client_dir.join("logs")]
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
