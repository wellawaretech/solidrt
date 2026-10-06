// The version manifest, shared by the go client's version store
// (src/go/store.rs) and the packed runner's folder boot (src/main.rs). The
// manifest's canonical form is the exact JSON string the CLI serialized
// (packages/cli/src/project.ts); it is parsed here but never re-serialized -
// the version id is the sha256 of those exact bytes.

use alloy::rendertree::FontPayload;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
// The three structs are manifest.json's schema in full; which fields a build
// reads is that build's business.
#[allow(dead_code)]
pub struct Manifest {
  #[serde(rename = "appId")]
  pub app_id: String,
  // org is pack-only (the folder has no trailer to carry identity; dev
  // manifests have no use for it yet). displayName is in both: the player
  // listing and the default window title read it (absent only in manifests
  // from CLIs that predate the field).
  #[serde(default)]
  pub org: Option<String>,
  #[serde(default, rename = "displayName")]
  pub display_name: Option<String>,
  // The app icon: an assets/ path into this manifest's asset set, SVG by
  // contract (the CLI validates the extension). Absent when the app declares
  // none.
  #[serde(default)]
  pub icon: Option<String>,
  // Provenance: the CLI release (or git describe of the checkout) that built
  // this version; "unknown" only for manifests from CLIs that predate the
  // field. The packed runner warns when it differs from its own version
  // (src/main.rs); runtimeVersion remains the compat gate proper.
  #[serde(default = "unknown_version", rename = "solidrtVersion")]
  pub solidrt_version: String,
  pub bundle: ManifestBundle,
  #[serde(default)]
  pub assets: Vec<AssetEntry>,
  #[serde(default)]
  pub fonts: Vec<FontRef>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct ManifestBundle {
  #[serde(default)]
  pub path: Option<String>,
  pub sha256: String,
  pub size: u64,
}

/// One collected assets/ file, as the manifest lists it.
#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct AssetEntry {
  pub path: String,
  pub sha256: String,
  pub size: u64,
}

/// A font annotation: an alias bound to an assets/ path registered at
/// startup, or to nothing (no path) when the app drops the runner's default
/// for that role (`"mono": false` in the project's font map).
#[derive(Deserialize)]
pub struct FontRef {
  pub alias: String,
  #[serde(default)]
  pub path: Option<String>,
}

/// An app's fonts as its manifest binds them: the files to register, and
/// every alias the manifest speaks for, bound or dropped, which a runner's
/// base set must not fill (see lattice::merge_fonts).
#[derive(Default)]
pub struct AppFonts {
  pub fonts: Vec<FontPayload>,
  pub aliases: Vec<String>,
}

pub(crate) fn unknown_version() -> String {
  "unknown".to_string()
}

/// Manifest paths land on disk as-is, so only plain forward-slash relative
/// paths inside assets/ (the project's asset folder) or isolates/ (isolate
/// bundles) are acceptable; anything else means a malformed or hostile
/// manifest.
pub fn safe_asset_path(path: &str) -> bool {
  (path.starts_with("assets/") || path.starts_with("isolates/"))
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
        log::warn!("[sol] {} in {}", e, dir.display());
        None
      }
    }
  }

  /// Load the font files this manifest annotates, relative to `dir`. A missing
  /// or unreadable font degrades to "not registered" (its role falls back)
  /// rather than failing the boot; its alias stays claimed either way, as
  /// the manifest bound it.
  pub fn load_fonts(&self, dir: &Path) -> AppFonts {
    let mut app = AppFonts::default();
    for font in &self.fonts {
      app.aliases.push(font.alias.clone());
      let Some(path) = font.path.as_deref().filter(|p| safe_asset_path(p)) else {
        continue;
      };
      match std::fs::read(dir.join(path)) {
        Ok(bytes) => {
          app.fonts.push(FontPayload { alias: Some(font.alias.clone()), bytes: std::borrow::Cow::Owned(bytes) })
        }
        Err(e) => log::warn!("[sol] Could not read font {path}: {e}"),
      }
    }
    app
  }
}
