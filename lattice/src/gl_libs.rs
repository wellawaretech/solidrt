// GL libraries carried inside a single-file packed executable (trailer kind 3,
// see src/main.rs and packages/cli/src/packer.ts). The OS loader cannot load a
// library from memory, so they are materialized into the packed app's cache
// tree and preloaded from there before SDL initializes. On Windows SDL's later
// LoadLibrary-by-name resolves to the already-loaded modules; dyld does not
// match a bare leaf name against an image loaded from another path, so on
// macOS SDL is additionally pointed at the extracted files through its
// library-path hints. Only Windows and macOS ship GL libraries (ANGLE) with
// the runner; elsewhere this module is a no-op and the CLI embeds nothing.
//
// Everything here fails soft: on any error the libraries simply are not
// preloaded, window creation fails as it would have anyway, and alloy's error
// message names the missing libraries.

/// Extract `libs` (filename, bytes) into `<pref SolidRT/app_id>/cache/gl/` and
/// preload them in order. Order is the CLI's contract: libGLESv2 first, so
/// libEGL's import of it resolves against the loaded module.
#[cfg(any(windows, target_os = "macos"))]
pub fn provision(app_id: &str, libs: &[(String, Vec<u8>)]) {
  if libs.is_empty() {
    return;
  }
  let app_id = if crate::storage::safe_component(app_id) { app_id } else { "default" };
  let dir = match alloy::sdl3::filesystem::get_pref_path("SolidRT", app_id) {
    Ok(dir) => dir.join("cache").join("gl"),
    Err(e) => {
      log::warn!("[srt] no writable pref path for GL libraries: {e}");
      return;
    }
  };
  if let Err(e) = std::fs::create_dir_all(&dir) {
    log::warn!("[srt] cannot create GL library dir {}: {e}", dir.display());
    return;
  }
  for (name, bytes) in libs {
    // The CLI only writes plain filenames, but the trailer is untrusted input:
    // never let a name traverse out of the tree.
    if !crate::storage::safe_component(name) {
      log::warn!("[srt] ignoring GL library with unsafe name {name:?}");
      continue;
    }
    let path = dir.join(name);
    // Refresh only on change so steady-state boots are a read, not a write.
    let stale = std::fs::read(&path).map(|current| current != *bytes).unwrap_or(true);
    if stale {
      if let Err(e) = std::fs::write(&path, bytes) {
        log::warn!("[srt] cannot write GL library {}: {e}", path.display());
        continue;
      }
    }
    match unsafe { libloading::Library::new(&path) } {
      // Keep the module loaded for the life of the process; there is no
      // meaningful unload point for a GL driver.
      Ok(lib) => std::mem::forget(lib),
      Err(e) => {
        log::warn!("[srt] cannot preload GL library {}: {e}", path.display());
        continue;
      }
    }
    #[cfg(target_os = "macos")]
    if let Some(hint) = sdl_library_hint(name) {
      alloy::sdl3::hint::set(hint, &path.to_string_lossy());
    }
  }
}

/// The SDL hint that names the full path of a GL library, so SDL loads exactly
/// the extracted file instead of dlopen-ing a bare name (SDL_HINT_EGL_LIBRARY /
/// SDL_HINT_OPENGL_LIBRARY).
#[cfg(target_os = "macos")]
fn sdl_library_hint(name: &str) -> Option<&'static str> {
  match name {
    "libEGL.dylib" => Some("SDL_EGL_LIBRARY"),
    "libGLESv2.dylib" => Some("SDL_OPENGL_LIBRARY"),
    _ => None,
  }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn provision(_app_id: &str, _libs: &[(String, Vec<u8>)]) {}