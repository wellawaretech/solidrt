// Version store under an app dir (okf/plans/client-storage-updates.md,
// stages 2 + 3):
//
//   apps/<app-id>/
//     versions/<manifest-hash>/   manifest.json + bundle.js + assets/...
//     state.json                  {version, current, previous, healthy, launches}
//
// A dev push is installed here before the engine reload applies it, so the
// app appears in the launcher's list and can be launched offline. The version
// id is the sha256 of the
// manifest's canonical bytes (the exact string the CLI sent, never
// re-serialized); every file's own hash and size are verified against its
// manifest entry before anything is written. Assets already held by the
// current or previous version (same path + hash per their manifests) are
// hardlinked into the new version; the caller fetches the rest
// (`missing_assets` -> `install`). healthy/launches are written but not yet
// acted on (health/rollback is stage 4).
//
// go-only for now: packed apps receive no installs until OTA (stage 4). The
// manifest types live in crate::manifest, shared with the packed runner's
// folder boot (serde_json is in every build since stage 3b).

use crate::manifest::{safe_asset_path, unknown_version, AssetEntry, Manifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const STATE_VERSION: u32 = 1;
// Dev retention: versions kept per app after an install (current and previous
// are always among them).
const KEEP_VERSIONS: usize = 5;

#[derive(Serialize, Deserialize)]
pub struct State {
  pub version: u32,
  // Version ids (manifest hashes). `previous` is the rollback target once
  // health tracking lands (stage 4).
  pub current: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub previous: Option<String>,
  pub healthy: bool,
  pub launches: u32,
}

fn sha256_hex(bytes: &[u8]) -> String {
  let digest = Sha256::digest(bytes);
  let mut hex = String::with_capacity(digest.len() * 2);
  for byte in digest {
    hex.push_str(&format!("{byte:02x}"));
  }
  hex
}

/// The asset entries a push still needs bytes for: listed in the manifest but
/// not held (same path + hash) by the current or previous version. Returns
/// them with the app id; an already-installed version needs nothing.
pub fn missing_assets(manifest: &str) -> Result<(String, Vec<AssetEntry>), String> {
  let store = crate::storage::get().ok_or("no writable storage")?;
  let parsed = Manifest::parse(manifest)?;
  let app_dir = store.app_dir(&parsed.app_id);
  if app_dir.join("versions").join(sha256_hex(manifest.as_bytes())).is_dir() {
    return Ok((parsed.app_id, Vec::new()));
  }
  let held = held_assets(&app_dir);
  let missing = parsed
    .assets
    .iter()
    .filter(|a| !held.get(&a.path).is_some_and(|(hash, _)| hash.eq_ignore_ascii_case(&a.sha256)))
    .cloned()
    .collect();
  Ok((parsed.app_id, missing))
}

/// Install a pushed version into the client store: the bundle from the push,
/// assets from `fetched` (keyed by manifest path) or hardlinked from the
/// versions already held. Returns the app id the version was installed under.
pub fn install(manifest: &str, code: &str, fetched: &HashMap<String, Vec<u8>>) -> Result<String, String> {
  let store = crate::storage::get().ok_or("no writable storage")?;
  let parsed = Manifest::parse(manifest)?;
  install_at(&store.app_dir(&parsed.app_id), manifest, code, fetched)?;
  Ok(parsed.app_id)
}

/// A stored version resolved for boot: the code to run. (The assets mount
/// base and fonts are resolved separately - `current_version_dir` and
/// `app_fonts` - when the reload applies.)
pub struct BootVersion {
  pub app_id: String,
  pub code: String,
}

/// An app's current version resolved for boot, or None when the id is unsafe,
/// the store is missing, or the app has no installed version.
pub fn load(app_id: &str) -> Option<BootVersion> {
  let version_dir = current_version_dir(app_id)?;
  let code = std::fs::read_to_string(version_dir.join("bundle.js")).ok()?;
  Some(BootVersion { app_id: app_id.to_string(), code })
}

/// The current installed version's manifest-annotated fonts, ready to
/// register. Empty when nothing is installed or the manifest names none; a
/// font file that fails to read is skipped (load_fonts warns), so its role
/// falls back rather than blocking the app.
pub fn app_fonts(app_id: &str) -> Vec<alloy::rendertree::FontPayload> {
  let Some(version_dir) = current_version_dir(app_id) else { return Vec::new() };
  let Some(manifest) = Manifest::load(&version_dir) else { return Vec::new() };
  manifest
    .load_fonts(&version_dir)
    .into_iter()
    .map(|(alias, bytes)| alloy::rendertree::FontPayload { alias: Some(alias), bytes: std::borrow::Cow::Owned(bytes) })
    .collect()
}

/// The current installed version's manifest-declared icon (SVG source), if
/// any. Same degradation as the listing: any failure means None.
pub fn app_icon(app_id: &str) -> Option<String> {
  let version_dir = current_version_dir(app_id)?;
  let manifest = Manifest::load(&version_dir)?;
  load_icon(&version_dir, &manifest)
}

/// The current installed version dir for an app, if the store has one. This is
/// what the assets mount points at while the app runs. Unsafe ids resolve to
/// nothing rather than following app_dir's fallback to "default".
pub fn current_version_dir(app_id: &str) -> Option<PathBuf> {
  if !crate::storage::safe_component(app_id) {
    return None;
  }
  let store = crate::storage::get()?;
  let app_dir = store.app_dir(app_id);
  let state = load_state(&app_dir)?;
  let dir = app_dir.join("versions").join(&state.current);
  dir.is_dir().then_some(dir)
}

/// An installed app as the launcher lists it.
pub struct InstalledApp {
  pub id: String,
  /// The installed manifest's displayName, defaulting to the id.
  pub name: String,
  /// The manifest-declared icon's SVG source, read at list time. None when
  /// the app declares no icon or the file is missing, oversized or unreadable.
  pub icon: Option<String>,
  /// The current version id (manifest hash).
  pub version: String,
  /// When the current version became current, in milliseconds since the epoch
  /// (0 when the store's timestamp is unreadable). This is state.json's mtime:
  /// it is rewritten exactly once per install that changes the current
  /// version, so a repush of an identical manifest leaves it alone.
  pub updated: u64,
  /// The current version's manifest-declared size: the bundle plus its
  /// assets. Claimed, not walked - the listing must stay cheap enough to run
  /// per row, and install verifies the claims against disk anyway.
  pub size: u64,
}

/// The installed apps under the client's apps root, most recently updated
/// first.
pub fn list_installed() -> Vec<InstalledApp> {
  match crate::storage::get().and_then(|s| s.apps_root()) {
    Some(apps) => list_installed_at(&apps),
    None => Vec::new(),
  }
}

// Dirs under an apps root whose state.json points at an existing version dir.
// Anchor-only dirs (a data sandbox for an app that was never installed) are
// not installed apps and are not listed.
pub(crate) fn list_installed_at(apps: &Path) -> Vec<InstalledApp> {
  let Ok(entries) = std::fs::read_dir(apps) else { return Vec::new() };
  let mut installed: Vec<InstalledApp> = entries
    .flatten()
    .filter_map(|entry| {
      let id = entry.file_name().to_str()?.to_string();
      let app_dir = entry.path();
      let state = load_state(&app_dir)?;
      let version_dir = app_dir.join("versions").join(&state.current);
      if !version_dir.is_dir() {
        return None;
      }
      let updated = modified_millis(&app_dir.join("state.json"));
      let manifest = Manifest::load(&version_dir);
      let size = manifest
        .as_ref()
        .map(|m| m.bundle.size + m.assets.iter().map(|a| a.size).sum::<u64>())
        .unwrap_or(0);
      let icon = manifest.as_ref().and_then(|m| load_icon(&version_dir, m));
      let name = manifest.and_then(|m| m.display_name).unwrap_or_else(|| id.clone());
      Some(InstalledApp { id, name, icon, version: state.current, updated, size })
    })
    .collect();
  // Newest first, the way a launcher list reads; equal timestamps (two
  // installs inside the filesystem's mtime granularity) fall back to name.
  installed.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id)));
  installed
}

// Icons ride along in every list() row, so an absurdly large file must not
// balloon the listing; anything real is a few KB.
const ICON_MAX_BYTES: u64 = 128 * 1024;

// The manifest-declared icon's SVG source from a version dir. Cosmetic, so
// every failure degrades to None (the launcher falls back) rather than
// failing the listing; only the oversize case warns, since it means a
// manifest that passed validation with an unreasonable icon.
fn load_icon(version_dir: &Path, manifest: &Manifest) -> Option<String> {
  let path = manifest.icon.as_deref().filter(|p| safe_asset_path(p))?;
  let file = version_dir.join(path);
  let len = std::fs::metadata(&file).ok()?.len();
  if len > ICON_MAX_BYTES {
    log::warn!("[srt] Ignoring icon {path}: {len} bytes exceeds the {ICON_MAX_BYTES} byte cap");
    return None;
  }
  std::fs::read_to_string(&file).ok()
}

// A file's mtime in milliseconds since the epoch, 0 when it cannot be read or
// predates the epoch. Informational, so an unreadable stat degrades to "unknown"
// rather than failing the listing.
fn modified_millis(path: &Path) -> u64 {
  std::fs::metadata(path)
    .and_then(|m| m.modified())
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

/// Full uninstall: the app's entire folder (versions, state.json and the data
/// sandbox).
pub fn remove_app(app_id: &str) -> Result<(), String> {
  let store = crate::storage::get().ok_or("no writable storage")?;
  let apps = store.apps_root().ok_or("no apps root in this layout")?;
  remove_app_at(&apps, app_id)
}

// Validates the id itself and joins it directly: app_dir's fallback to
// "default" on an unsafe id must never redirect a delete.
pub(crate) fn remove_app_at(apps: &Path, app_id: &str) -> Result<(), String> {
  if !crate::storage::safe_component(app_id) {
    return Err(format!("invalid app id {app_id:?}"));
  }
  let app_dir = apps.join(app_id);
  if !app_dir.is_dir() {
    return Err(format!("app {app_id} is not installed"));
  }
  std::fs::remove_dir_all(&app_dir).map_err(|e| format!("remove {app_id}: {e}"))
}

/// A stored version as the detail view lists it.
pub struct VersionInfo {
  /// The version id (manifest hash).
  pub id: String,
  /// Bytes on disk under the version dir. Assets shared between versions via
  /// hardlinks count in every version holding them.
  pub size: u64,
  pub current: bool,
  /// The version manifest's solidrtVersion (the CLI release that built it;
  /// "unknown" when the manifest predates the field or is unreadable).
  pub solidrt_version: String,
}

/// One file in a listing: a relative path and its size in bytes.
pub struct FileEntry {
  pub path: String,
  pub size: u64,
}

/// One cached fetch as the detail view aggregates it: the (resolved) url,
/// the response content type (when stored) and the entry's size on disk.
pub struct CacheEntry {
  pub url: String,
  pub content_type: Option<String>,
  pub size: u64,
}

/// Usage details for one installed app, as the launcher's detail view shows
/// them. The listings are disk walks, not manifest claims: the manifest is
/// reconciled against disk at install time, and what the view shows is what
/// is actually there.
pub struct AppInfo {
  pub id: String,
  pub name: String,
  /// The current version id.
  pub version: String,
  /// Installed versions, current first, then newest first (by dir mtime).
  pub versions: Vec<VersionInfo>,
  /// Total bytes under versions/.
  pub install_size: u64,
  /// Total bytes under the data sandbox.
  pub data_size: u64,
  /// The current version dir's actual files on disk (recursive, sorted).
  pub version_files: Vec<FileEntry>,
  /// The data sandbox's actual files on disk (recursive, sorted).
  pub data_files: Vec<FileEntry>,
  /// Total bytes under the app's fetch cache.
  pub cache_size: u64,
  /// The fetch cache's committed entries (sorted by url).
  pub cache: Vec<CacheEntry>,
}

/// Usage details for an installed app. Errors on invalid or uninstalled ids.
pub fn app_info(app_id: &str) -> Result<AppInfo, String> {
  let store = crate::storage::get().ok_or("no writable storage")?;
  let apps = store.apps_root().ok_or("no apps root in this layout")?;
  app_info_at(&apps, app_id)
}

// Validates the id itself and joins it directly, like remove_app_at.
pub(crate) fn app_info_at(apps: &Path, app_id: &str) -> Result<AppInfo, String> {
  if !crate::storage::safe_component(app_id) {
    return Err(format!("invalid app id {app_id:?}"));
  }
  let app_dir = apps.join(app_id);
  let state = load_state(&app_dir).ok_or_else(|| format!("app {app_id} is not installed"))?;
  let versions_root = app_dir.join("versions");

  let mut versions: Vec<(std::time::SystemTime, VersionInfo)> = Vec::new();
  if let Ok(entries) = std::fs::read_dir(&versions_root) {
    for entry in entries.flatten() {
      let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
      let path = entry.path();
      if !path.is_dir() || name.starts_with(".tmp-") {
        continue;
      }
      let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
      let solidrt_version = Manifest::load(&path).map(|m| m.solidrt_version).unwrap_or_else(unknown_version);
      versions.push((
        modified,
        VersionInfo { current: name == state.current, id: name, size: dir_size(&path), solidrt_version },
      ));
    }
  }
  if !versions.iter().any(|(_, v)| v.current) {
    return Err(format!("app {app_id} is not installed"));
  }
  versions.sort_by(|a, b| b.1.current.cmp(&a.1.current).then_with(|| b.0.cmp(&a.0)));
  let install_size = versions.iter().map(|(_, v)| v.size).sum();
  let versions: Vec<VersionInfo> = versions.into_iter().map(|(_, v)| v).collect();

  let current_dir = versions_root.join(&state.current);
  let manifest = Manifest::load(&current_dir);
  let name = manifest.and_then(|m| m.display_name).unwrap_or_else(|| app_id.to_string());
  let version_files = collect_files(&current_dir);
  let data_files = collect_files(&app_dir.join("data"));
  let data_size = data_files.iter().map(|e| e.size).sum();
  let cache = cache_entries(&app_dir.join("cache"));
  let cache_size = cache.iter().map(|e| e.size).sum();

  Ok(AppInfo {
    id: app_id.to_string(),
    name,
    version: state.current,
    versions,
    install_size,
    data_size,
    version_files,
    data_files,
    cache_size,
    cache,
  })
}

// Every file under `root` as a relative path + size, sorted by path.
// Unreadable entries are skipped (the listing is informational).
/// Delete an app's fetch cache. A missing dir is fine: clearing an empty
/// cache is a no-op. The id is validated but not required to be installed,
/// so a leftover cache of a removed app is still clearable.
pub fn clear_cache(app_id: &str) -> Result<(), String> {
  if !crate::storage::safe_component(app_id) {
    return Err(format!("invalid app id {app_id:?}"));
  }
  let store = crate::storage::get().ok_or("no writable storage")?;
  let dir = store.cache_dir(app_id);
  if !dir.exists() {
    return Ok(());
  }
  std::fs::remove_dir_all(&dir).map_err(|e| format!("clear cache {app_id}: {e}"))
}

// The app's fetch-cache entries, sorted by url. Meta blobs that are not
// fetch entries are skipped.
fn cache_entries(dir: &Path) -> Vec<CacheEntry> {
  let mut entries: Vec<CacheEntry> = forge::cache::scan(dir)
    .into_iter()
    .filter_map(|e| {
      let meta = forge::fetch::cached_meta(&e.meta)?;
      Some(CacheEntry { url: meta.url, content_type: meta.content_type, size: e.size })
    })
    .collect();
  entries.sort_by(|a, b| a.url.cmp(&b.url));
  entries
}

fn collect_files(root: &Path) -> Vec<FileEntry> {
  fn walk(dir: &Path, prefix: &str, out: &mut Vec<FileEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
      let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
      let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
      match entry.metadata() {
        Ok(meta) if meta.is_dir() => walk(&entry.path(), &path, out),
        Ok(meta) => out.push(FileEntry { path, size: meta.len() }),
        Err(_) => {}
      }
    }
  }
  let mut out = Vec::new();
  walk(root, "", &mut out);
  out.sort_by(|a, b| a.path.cmp(&b.path));
  out
}

// Recursive size in bytes; unreadable entries count as zero (the figures are
// informational, not accounting).
fn dir_size(path: &Path) -> u64 {
  let Ok(entries) = std::fs::read_dir(path) else { return 0 };
  entries
    .flatten()
    .map(|entry| match entry.metadata() {
      Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
      Ok(meta) => meta.len(),
      Err(_) => 0,
    })
    .sum()
}

// The assets held by the versions state.json points at (current first, so its
// copy wins), as path -> (manifest hash, on-disk file). Trusts the stored
// manifests: version dirs are immutable once committed.
fn held_assets(app_dir: &Path) -> HashMap<String, (String, PathBuf)> {
  let mut held = HashMap::new();
  let Some(state) = load_state(app_dir) else { return held };
  for id in std::iter::once(&state.current).chain(state.previous.as_ref()) {
    let version_dir = app_dir.join("versions").join(id);
    let Some(parsed) = Manifest::load(&version_dir) else { continue };
    for asset in parsed.assets {
      if !safe_asset_path(&asset.path) {
        continue;
      }
      let file = version_dir.join(&asset.path);
      if !held.contains_key(&asset.path) && file.is_file() {
        held.insert(asset.path, (asset.sha256, file));
      }
    }
  }
  held
}

/// Verify + write one version and point state.json at it. Pure with respect to
/// globals (tests drive it against a temp dir). Returns the version id.
pub(crate) fn install_at(
  app_dir: &Path,
  manifest: &str,
  code: &str,
  fetched: &HashMap<String, Vec<u8>>,
) -> Result<String, String> {
  let parsed = Manifest::parse(manifest)?;
  if !parsed.bundle.sha256.eq_ignore_ascii_case(&sha256_hex(code.as_bytes())) {
    return Err("bundle hash does not match its manifest".to_string());
  }
  if parsed.bundle.size != code.len() as u64 {
    return Err("bundle size does not match its manifest".to_string());
  }
  // Dev pushes always name the bundle "bundle.js"; other names are a stage-4
  // (OTA) concern and refused until then, so path and file cannot disagree.
  if parsed.bundle.path.as_deref().is_some_and(|p| p != "bundle.js") {
    return Err(format!("unsupported bundle path {:?}", parsed.bundle.path.as_deref().unwrap_or_default()));
  }

  let version = sha256_hex(manifest.as_bytes());
  let version_dir = app_dir.join("versions").join(&version);
  if !version_dir.is_dir() {
    // Stage the version dir next to its final name, then rename: a crash
    // mid-write leaves only a .tmp- leftover (cleaned by the next prune),
    // never a half-written version state.json could point at.
    let tmp = app_dir.join("versions").join(format!(".tmp-{version}"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("create version dir: {e}"))?;
    std::fs::write(tmp.join("manifest.json"), manifest).map_err(|e| format!("write manifest: {e}"))?;
    std::fs::write(tmp.join("bundle.js"), code).map_err(|e| format!("write bundle: {e}"))?;
    let held = held_assets(app_dir);
    for asset in &parsed.assets {
      if !safe_asset_path(&asset.path) {
        return Err(format!("unsafe asset path {}", asset.path));
      }
      let dest = tmp.join(&asset.path);
      if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create asset dir: {e}"))?;
      }
      if let Some(bytes) = fetched.get(&asset.path) {
        if !asset.sha256.eq_ignore_ascii_case(&sha256_hex(bytes)) || asset.size != bytes.len() as u64 {
          return Err(format!("asset {} does not match its manifest", asset.path));
        }
        std::fs::write(&dest, bytes).map_err(|e| format!("write asset {}: {e}", asset.path))?;
      } else if let Some((_, src)) = held.get(&asset.path).filter(|(h, _)| h.eq_ignore_ascii_case(&asset.sha256)) {
        // Manifest-diff reuse: same path + hash as a held version shares the
        // file (versions are immutable, so a shared inode is safe).
        if std::fs::hard_link(src, &dest).is_err() {
          std::fs::copy(src, &dest).map_err(|e| format!("copy asset {}: {e}", asset.path))?;
        }
      } else {
        return Err(format!("asset {} was neither fetched nor held", asset.path));
      }
    }
    std::fs::rename(&tmp, &version_dir).map_err(|e| format!("commit version dir: {e}"))?;
  }

  let previous = match load_state(app_dir) {
    // Repush of the current version (same manifest bytes): nothing changes.
    Some(state) if state.current == version => return Ok(version),
    Some(state) => Some(state.current),
    None => None,
  };
  let state = State { version: STATE_VERSION, current: version.clone(), previous, healthy: true, launches: 0 };
  save_state(app_dir, &state)?;
  prune(app_dir, &state);
  Ok(version)
}

/// The current installed bundle's code, or None when the store is empty or
/// unreadable (callers fall back to the next boot source). Only tests read
/// through this today; `load` resolves the richer BootVersion.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn load_current_at(app_dir: &Path) -> Option<String> {
  let state = load_state(app_dir)?;
  std::fs::read_to_string(app_dir.join("versions").join(&state.current).join("bundle.js")).ok()
}

fn load_state(app_dir: &Path) -> Option<State> {
  let bytes = std::fs::read(app_dir.join("state.json")).ok()?;
  let state: State = match serde_json::from_slice(&bytes) {
    Ok(state) => state,
    Err(e) => {
      log::warn!("[sgo] state.json parse failed ({e}); ignoring store");
      return None;
    }
  };
  // Version ids are used as path components; anything else means a tampered
  // or corrupt state file, so treat the store as empty rather than follow it.
  let hex = |s: &String| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit());
  if !hex(&state.current) || !state.previous.as_ref().map(hex).unwrap_or(true) {
    log::warn!("[sgo] state.json has malformed version ids; ignoring store");
    return None;
  }
  Some(state)
}

fn save_state(app_dir: &Path, state: &State) -> Result<(), String> {
  let json = serde_json::to_vec_pretty(state).map_err(|e| format!("state serialize: {e}"))?;
  let path = app_dir.join("state.json");
  let tmp = app_dir.join("state.json.tmp");
  std::fs::write(&tmp, &json).map_err(|e| format!("state write: {e}"))?;
  std::fs::rename(&tmp, &path).map_err(|e| format!("state rename: {e}"))?;
  Ok(())
}

/// Retention: keep current + previous plus the newest other versions up to
/// KEEP_VERSIONS total, drop the rest and any stale .tmp- staging dirs.
/// Install order is approximated by dir mtime, which is fine for dev pruning.
fn prune(app_dir: &Path, state: &State) {
  let versions = app_dir.join("versions");
  let Ok(entries) = std::fs::read_dir(&versions) else { return };

  let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
  for entry in entries.flatten() {
    let name = entry.file_name();
    let Some(name) = name.to_str() else { continue };
    if name == state.current || Some(name) == state.previous.as_deref() {
      continue;
    }
    let path = entry.path();
    if !path.is_dir() {
      continue;
    }
    if name.starts_with(".tmp-") {
      let _ = std::fs::remove_dir_all(&path);
      continue;
    }
    let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    candidates.push((modified, path));
  }

  let protected = 1 + state.previous.is_some() as usize;
  let keep = KEEP_VERSIONS.saturating_sub(protected);
  if candidates.len() <= keep {
    return;
  }
  candidates.sort_by(|a, b| b.0.cmp(&a.0));
  for (_, path) in candidates.drain(keep..) {
    if let Err(e) = std::fs::remove_dir_all(&path) {
      log::warn!("[sgo] prune of {} failed: {e}", path.display());
    }
  }
}
