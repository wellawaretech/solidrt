//! The OS layer: every facility the platform provides, one contract each.
//!
//! `ore` is the platform boundary (okf/design/platform-layering.md). It
//! wraps SDL where SDL reaches and our own native code (JNI, Win32,
//! Objective-C, D-Bus) where it stops; SDL is its implementation detail.
//! It depends on nothing of ours, and names no scripting-engine types: the
//! hosts (lattice) and the marshalling (flux `ore_plugins/`) call it.
//!
//! Its first facilities are window-free: the main-thread bridge, the
//! links (opening one, the app's own scheme registration and its
//! single-instance hand-off), the locale, the share sheet, the
//! clipboard, the user folders, the power state and thread placement.
//! Its window part, behind the `window` feature (what needs SDL's video
//! subsystem), holds the system theme. On Android it publishes the JavaVM
//! and the activity and is the JNI way into the activity (`android`). The
//! window, the event pump and the devices move in from alloy, forge and
//! lattice as features touch them, not before.

#[cfg(target_os = "android")]
pub mod android;
pub mod clipboard;
pub mod folders;
pub mod link;
pub mod locale;
pub mod main_thread;
pub mod permission;
pub mod power;
pub mod share;
#[cfg(feature = "window")]
pub mod theme;
pub mod threads;

#[cfg(test)]
mod tests;

/// SDL's last error on this thread, as text. `sdl3::get_error` panics on
/// a message that is not UTF-8; this one replaces the bad bytes.
pub(crate) fn sdl_error() -> String {
  // SAFETY: SDL_GetError never returns null; the string it points to is
  // SDL's thread-local buffer, valid until the next SDL call on this
  // thread, and copied out before that.
  unsafe { std::ffi::CStr::from_ptr(sdl3::sys::error::SDL_GetError()) }.to_string_lossy().into_owned()
}
