//! The per-user locations: the app's preference directory and the user's
//! home. SDL's filesystem functions are safe on any thread and need no
//! SDL_Init, so headless callers (forge, flux without a window) use them
//! too.

use std::path::PathBuf;

/// The OS-designated per-user directory for `org`'s `app`, where an app
/// writes its own data; SDL creates it when it is missing. The error is
/// SDL's text when the platform has no writable location.
pub fn pref_dir(org: &str, app: &str) -> Result<PathBuf, String> {
  sdl3::filesystem::get_pref_path(org, app).map_err(|e| e.to_string())
}

/// The current user's home directory, or `None` when the environment does not
/// name one (HOME on unix, USERPROFILE on Windows).
pub fn home_dir() -> Option<String> {
  let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
  std::env::var(var).ok().filter(|v| !v.is_empty())
}
