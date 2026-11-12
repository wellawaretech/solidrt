//! The OS-level dark or light preference. Part of ore's window part (the
//! `window` feature): `SDL_GetSystemTheme` returns the value the video
//! device caches, so this reads `Unknown` when no video subsystem is
//! initialized. It is not a window-free fact; a change event comes with
//! the event pump.

use sdl3::sys::video::{SDL_GetSystemTheme, SDL_SystemTheme};

/// Mirrored into a local enum: the sdl3 crate wraps SDL_GetSystemTheme,
/// but its SystemTheme derives nothing (not even Clone), so it cannot ride
/// in an event value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemTheme {
  Dark,
  Light,
  Unknown,
}

/// The system theme the video subsystem last saw; `Unknown` without one.
pub fn system_theme() -> SystemTheme {
  // SAFETY: SDL_GetSystemTheme takes no arguments and only reads the video
  // device's cached theme, or answers UNKNOWN when there is no device.
  match unsafe { SDL_GetSystemTheme() } {
    SDL_SystemTheme::DARK => SystemTheme::Dark,
    SDL_SystemTheme::LIGHT => SystemTheme::Light,
    _ => SystemTheme::Unknown,
  }
}
