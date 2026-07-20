// Version store under an app dir (okf/plans/client-storage-updates.md, stage 2):
//
//   apps/<app-id>/
//     versions/<manifest-hash>/   manifest.json + bundle.js
//     state.json                  {version, current, previous, healthy, launches}
//
// A dev push is installed here before the engine reload applies it, so the
// client can relaunch the app offline. The version id is the sha256 of the
// manifest's canonical bytes (the exact string the CLI sent, never
// re-serialized); the bundle's own hash and size are verified against the
// manifest entry before anything is written. healthy/launches are written but
// not yet acted on (health/rollback is stage 4).
//
// go-only for now: packed apps receive no installs until OTA (stage 4), and
// lifting this into every build re-opens the serde-free question for the
// packed runner.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

#[derive(Deserialize)]
struct Manifest {
  #[serde(rename = "appId")]
  app_id: String,
  bundle: ManifestBundle,
}

#[derive(Deserialize)]
struct ManifestBundle {
  sha256: String,
  size: u64,
}

fn sha256_hex(bytes: &[u8]) -> String {
  let digest = Sha256::digest(bytes);
  let mut hex = String::with_capacity(digest.len() * 2);
  for byte in digest {
    hex.push_str(&format!("{byte:02x}"));
  }
  hex
}

/// Install a pushed bundle into the client store. Returns the app id the
/// version was installed under (from the manifest).
pub fn install(manifest: &str, code: &str) -> Result<String, String> {
  let store = crate::storage::get().ok_or("no writable storage")?;
  let parsed: Manifest = serde_json::from_str(manifest).map_err(|e| format!("manifest parse failed: {e}"))?;
  install_at(&store.app_dir(&parsed.app_id), manifest, code)?;
  Ok(parsed.app_id)
}

/// The last-installed app and its current bundle, for offline boot.
pub fn load_last() -> Option<(String, String)> {
  let app_id = super::config::load().last_app?;
  let store = crate::storage::get()?;
  let code = load_current_at(&store.app_dir(&app_id))?;
  Some((app_id, code))
}

/// Verify + write one version and point state.json at it. Pure with respect to
/// globals (tests drive it against a temp dir). Returns the version id.
pub(crate) fn install_at(app_dir: &Path, manifest: &str, code: &str) -> Result<String, String> {
  let parsed: Manifest = serde_json::from_str(manifest).map_err(|e| format!("manifest parse failed: {e}"))?;
  if !parsed.bundle.sha256.eq_ignore_ascii_case(&sha256_hex(code.as_bytes())) {
    return Err("bundle hash does not match its manifest".to_string());
  }
  if parsed.bundle.size != code.len() as u64 {
    return Err("bundle size does not match its manifest".to_string());
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
/// unreadable (callers fall back to the next boot source).
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
