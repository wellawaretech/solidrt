// The version manifest, shared by the go client's version store
// (src/go/store.rs) and the packed runner's folder boot (src/main.rs). The
// manifest's canonical form is the exact JSON string the CLI serialized
// (packages/cli/src/project.ts); it is parsed here but never re-serialized -
// the version id is the sha256 of those exact bytes.

use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct Manifest {
  #[serde(rename = "appId")]
  pub app_id: String,
  // Present in pack manifests (the folder has no trailer to carry identity);
  // dev manifests omit them and default from the app id.
  #[serde(default)]
  pub org: Option<String>,
  #[serde(default, rename = "displayName")]
  pub display_name: Option<String>,
  // Provenance: the CLI release that built this version ("unknown" from an
  // in-repo CLI, and defaulted for manifests from CLIs that predate the
  // field). Informational, unlike runtimeVersion's compat gate.
  #[serde(default = "unknown_version", rename = "solidrtVersion")]
  pub solidrt_version: String,
  pub bundle: ManifestBundle,
  #[serde(default)]
  pub assets: Vec<AssetEntry>,
  #[serde(default)]
  pub fonts: Vec<FontRef>,
}

#[derive(Deserialize)]
pub struct ManifestBundle {
  #[serde(default)]
  pub path: Option<String>,
  pub sha256: String,
  pub size: u64,
}

/// One collected assets/ file, as the manifest lists it.
#[derive(Deserialize, Clone)]
pub struct AssetEntry {
  pub path: String,
  pub sha256: String,
  pub size: u64,
}

/// A font annotation: an assets/ path registered under an alias at startup.
#[derive(Deserialize)]
pub struct FontRef {
  pub path: String,
  pub alias: String,
}

pub(crate) fn unknown_version() -> String {
  "unknown".to_string()
}

/// Manifest paths land on disk as-is, so only plain forward-slash relative
/// paths inside assets/ are acceptable; anything else means a malformed or
/// hostile manifest.
pub fn safe_asset_path(path: &str) -> bool {
  path.starts_with("assets/")
    && !path.contains('\\')
    && path.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

impl Manifest {
  /// Parse the canonical manifest string.
  pub fn parse(manifest: &str) -> Result<Manifest, String> {
    serde_json::from_str(manifest).map_err(|e| format!("manifest parse failed: {e}"))
  }

  /// Parse `manifest.json` inside a version dir (or a pack folder). None when
  /// absent or unreadable.
  pub fn load(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    match Manifest::parse(&text) {
      Ok(manifest) => Some(manifest),
      Err(e) => {
        log::warn!("[srt] {} in {}", e, dir.display());
        None
      }
    }
  }

  /// Load the font files this manifest annotates, relative to `dir`. A missing
  /// or unreadable font degrades to "not registered" (its role falls back)
  /// rather than failing the boot.
  pub fn load_fonts(&self, dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut fonts = Vec::new();
    for font in &self.fonts {
      if !safe_asset_path(&font.path) {
        continue;
      }
      match std::fs::read(dir.join(&font.path)) {
        Ok(bytes) => fonts.push((font.alias.clone(), bytes)),
        Err(e) => log::warn!("[srt] Could not read font {}: {e}", font.path),
      }
    }
    fonts
  }
}
