// Persistent dev-client config, stored as a single `config.json` in the app's
// per-user writable directory (SDL pref path, cross-platform incl. Android).
//
// One versioned struct holds everything the dev client remembers across runs;
// add fields here as more state needs persisting. Writers should load(), mutate,
// then save() so concurrent fields are not clobbered. Saves are atomic (write a
// temp file, then rename over the target) so a crash mid-write never leaves a
// half-written config behind.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Organization/application names for the SDL pref path. The dev client uses a
// distinct app name so its store never collides with a packed app's.
const ORG: &str = "SolidRT";
const APP: &str = "go";

// Bump when the on-disk shape changes incompatibly and add migration handling
// in load(). v1 is the initial layout.
const CONFIG_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Config {
  pub version: u32,
  // Recently connected dev-server addresses, most-recent-first.
  #[serde(default)]
  pub recents: Vec<String>,
}

impl Default for Config {
  fn default() -> Self {
    Config { version: CONFIG_VERSION, recents: Vec::new() }
  }
}

fn config_path() -> Option<PathBuf> {
  match alloy::sdl3::filesystem::get_pref_path(ORG, APP) {
    Ok(dir) => Some(dir.join("config.json")),
    Err(e) => {
      log::warn!("[sgo] no writable config dir: {e}");
      None
    }
  }
}

/// Load the persisted config, falling back to defaults if it is missing or
/// unreadable (a corrupt file is logged and ignored rather than fatal).
pub fn load() -> Config {
  let Some(path) = config_path() else { return Config::default() };
  match std::fs::read(&path) {
    Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
      log::warn!("[sgo] config.json parse failed ({e}); using defaults");
      Config::default()
    }),
    // Absent file on first run is normal; other errors are non-fatal too.
    Err(_) => Config::default(),
  }
}

/// Persist the config via a temp-file-then-rename so the target is never seen
/// half-written.
pub fn save(config: &Config) {
  let Some(path) = config_path() else { return };
  let json = match serde_json::to_vec_pretty(config) {
    Ok(j) => j,
    Err(e) => {
      log::warn!("[sgo] config serialize failed: {e}");
      return;
    }
  };
  let tmp = path.with_extension("json.tmp");
  if let Err(e) = std::fs::write(&tmp, &json) {
    log::warn!("[sgo] config write failed: {e}");
    return;
  }
  if let Err(e) = std::fs::rename(&tmp, &path) {
    log::warn!("[sgo] config rename failed: {e}");
  }
}

/// Convenience: persist just the recents list, preserving any other fields.
pub fn save_recents(recents: &[String]) {
  let mut config = load();
  config.recents = recents.to_vec();
  save(&config);
}
