//! The OS layer: every facility the platform provides, one contract each.
//!
//! `ore` is the platform boundary (okf/design/platform-layering.md). It
//! wraps SDL where SDL reaches and our own native code (JNI, Win32,
//! Objective-C, D-Bus) where it stops; SDL is its implementation detail.
//! It depends on nothing of ours, and names no scripting-engine types: the
//! hosts (lattice) and the marshalling (flux `ore_plugins/`) call it.
//!
//! The platform (`platform`: SDL's init and the hints that precede it, on
//! the host's main thread, the event pump's waker, the lifecycle watch and
//! the gamepads), the events the pump hands upward (`event`), on Android
//! the touch side channel (`touch`), the display's vsync source
//! (`display_vsync`, Android's Choreographer), the video plane beneath the
//! window (`video_plane`, Android's), the held redraw and its answer
//! (`redraw`, Android's surface asks), and the window-free facilities: the
//! main-thread bridge, the links (opening one, the app's own scheme
//! registration and its single-instance hand-off), the locale, the share
//! sheet, haptic feedback (`haptics`), the clipboard, the audio devices
//! (`audio`: the playback device, the converter, the recording device),
//! the camera (`camera`), the user folders, the power state, thread
//! placement, the clock (`time`), the log sink, and the platform
//! vocabulary of input, display, gamepad and lifecycle types. Its window
//! part, behind the `window` feature (what needs SDL's video subsystem),
//! holds the pump's wait itself (`Platform::wait`: its translation reads
//! the window), the window (`window`: opening it with the headless chain,
//! its commands, cursors, text input, input device presence and the
//! screen keyboard inset), the GL surface and its context-loss recreation
//! (`gl_surface`, with the GL library path), the system theme, the file
//! dialogs with their session (`dialog`), and the cursor and text input
//! vocabulary. On Android it publishes the JavaVM and the activity and is
//! the JNI way into the activity (`android`). forge's and lattice's OS code
//! moves in as features touch it, not before
//! (okf/design/platform-layering.md, Migration).

#[cfg(target_os = "android")]
pub mod android;
pub mod audio;
pub mod camera;
pub mod clipboard;
#[cfg(feature = "window")]
pub mod cursor;
#[cfg(feature = "window")]
pub mod dialog;
pub mod display;
pub mod display_vsync;
pub mod event;
pub mod folders;
pub mod gamepad;
#[cfg(feature = "window")]
pub mod gl_surface;
pub mod haptics;
pub mod input;
pub mod lifecycle;
pub mod link;
pub mod locale;
pub mod logging;
pub mod main_thread;
pub mod permission;
pub mod platform;
pub mod power;
pub mod redraw;
pub mod share;
#[cfg(feature = "window")]
pub mod text_input;
#[cfg(feature = "window")]
pub mod theme;
pub mod threads;
pub mod time;
#[cfg(target_os = "android")]
pub mod touch;
pub mod video_plane;
#[cfg(feature = "window")]
pub mod window;

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
