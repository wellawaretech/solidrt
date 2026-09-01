// The factory payload a packed distribution boots: the single-file trailer
// (the pack folder in section form, appended to the runner by `srt pack`), a
// standalone .srtapp (the same sections on their own, `srt pack --app`, named
// on the command line or stored inside an APK), or a folder next to the
// runner (`srt pack --folder`). One shape behind all
// (okf/plans/client-storage-updates.md, stage 3): the manifest defines the
// version's file set (the runner is deliberately unlisted) and carries the app
// identity; a .js bundle is JS source, anything else QuickJS bytecode; fonts
// come from the manifest annotations; assets resolve through the mount (a
// dir, or ranges inside the packed image).
//
// Trailer parsing lives in forge::trailer (shared with fluxrt); the layout is
// documented there and written by packages/cli/src/pack/trailer.ts. Sections
// here: 1 = the canonical manifest JSON, 2 = a manifest-listed file (name =
// its manifest path), 3 = a GL library (name = its filename; extracted and
// preloaded before window setup, see gl_libs.rs).
//
// Shared by the desktop runner (src/main.rs, which reads its own image or a
// .srtapp path) and the Android runtime (SDL_main in lib.rs, which reads the
// .srtapp at its offset inside the APK): the caller parses the trailer for
// its source, this module turns it into a bootable payload.

pub const EMBED_MAGIC: &[u8; 9] = b"SOLIDRT\x88\x44";

pub struct FactoryPayload {
  pub app: crate::AppSource,
  pub fonts: Vec<alloy::rendertree::FontPayload>,
  pub app_id: String,
  pub base: forge::fs::AssetsBase,
  // (filename, bytes) in section order; only the single-file trailer carries
  // these (a folder pack ships the libraries next to the runner instead, and
  // an APK carries them as jniLibs).
  pub gl_libs: Vec<(String, Vec<u8>)>,
}

// The app was built by one SolidRT version and runs on another. A warning,
// not a refusal: most differences (a checkout commit apart, a patch release)
// run fine, and one that does not (a bytecode format change) fails to load
// with QuickJS's own "invalid version" error right after this line, which
// then explains it. Same version: silent. Printed directly: payloads load
// before lattice::start installs the logger.
fn warn_version_mismatch(manifest: &crate::manifest::Manifest) {
  if manifest.solidrt_version != crate::VERSION {
    eprintln!("[srt] app built by SolidRT {}, runner is {}", manifest.solidrt_version, crate::VERSION);
  }
}

// A plain filename, as the manifest's bundle entry must be: it cannot reach
// outside its distribution.
fn plain_bundle_name(manifest: &crate::manifest::Manifest) -> Option<&str> {
  let name = manifest.bundle.path.as_deref().unwrap_or("bundle.js");
  (!name.contains('/') && !name.contains('\\') && !name.starts_with('.')).then_some(name)
}

fn app_from_bundle(name: &str, bytes: Vec<u8>) -> Option<crate::AppSource> {
  if name.ends_with(".js") {
    Some(crate::AppSource::Text(String::from_utf8(bytes).ok()?))
  } else {
    Some(crate::AppSource::Bytecode(bytes))
  }
}

/// Turn a parsed trailer into a bootable payload. Only the boot files
/// (manifest, bundle, fonts, GL libraries) are read here, each as a ranged
/// read; everything else stays in place behind the assets mount.
pub fn load(trailer: forge::trailer::Trailer) -> Option<FactoryPayload> {
  let manifest = trailer.sections.iter().find(|s| s.kind == forge::trailer::SECTION_MANIFEST)?;
  let manifest = String::from_utf8(trailer.section_bytes(manifest).ok()?).ok()?;
  let manifest = crate::manifest::Manifest::parse(&manifest).ok()?;
  warn_version_mismatch(&manifest);
  let index = trailer.file_index();
  let read_file = |name: &str| {
    let &(offset, len) = index.get(name)?;
    trailer.read_range(offset, len).ok()
  };
  let bundle_name = plain_bundle_name(&manifest)?;
  let app = app_from_bundle(bundle_name, read_file(bundle_name)?)?;
  let fonts = manifest
    .fonts
    .iter()
    .filter_map(|font| {
      let bytes = read_file(&font.path)?;
      Some(alloy::rendertree::FontPayload { alias: Some(font.alias.clone()), bytes: std::borrow::Cow::Owned(bytes) })
    })
    .collect();
  // Section order, which gl_libs.rs relies on for preload order.
  let gl_libs = trailer
    .sections
    .iter()
    .filter(|s| s.kind == forge::trailer::SECTION_GL_LIB && !s.name.is_empty())
    .filter_map(|s| Some((s.name.clone(), trailer.section_bytes(s).ok()?)))
    .collect();
  let exe = trailer.exe;
  Some(FactoryPayload { app, fonts, app_id: manifest.app_id, base: forge::fs::AssetsBase::Packed { exe, index }, gl_libs })
}

/// The trailer at the end of `path` (a packed executable's own image, or a
/// standalone .srtapp file), loaded as a payload.
pub fn load_path(path: std::path::PathBuf) -> Option<FactoryPayload> {
  load(forge::trailer::read(path, EMBED_MAGIC)?)
}

/// A folder distribution next to the runner: manifest.json + the bundle it
/// names + assets/. Absent or unreadable pieces degrade to "no folder payload".
pub fn load_adjacent_folder() -> Option<FactoryPayload> {
  let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
  let manifest = crate::manifest::Manifest::load(&dir)?;
  warn_version_mismatch(&manifest);
  let bundle_name = plain_bundle_name(&manifest)?;
  let app = app_from_bundle(bundle_name, std::fs::read(dir.join(bundle_name)).ok()?)?;
  let fonts = manifest
    .load_fonts(&dir)
    .into_iter()
    .map(|(alias, bytes)| alloy::rendertree::FontPayload { alias: Some(alias), bytes: std::borrow::Cow::Owned(bytes) })
    .collect();
  Some(FactoryPayload { app, fonts, app_id: manifest.app_id, base: forge::fs::AssetsBase::Dir(dir), gl_libs: Vec::new() })
}
