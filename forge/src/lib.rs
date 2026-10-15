//! Capability cores for a headless process.
//!
//! `forge` holds what a process can do without a window or a platform loop:
//! the HTTP server core, the engine-free halves of sqlite/p2p/subprocess/
//! fetch/fs/path/process/events/websocket, the decoders, the shared
//! byte-stream primitive, and the logging sink. OS-specific code lives here
//! too when it needs no SDL (AMediaCodec in `video`, the content providers in
//! `fs`, the terminal in `tty`): forge reaches the OS directly, on Android
//! through the process-wide JavaVM and activity the platform (alloy)
//! publishes in ndk-context at startup. What needs SDL (the window, the loop
//! thread, a device subsystem) is alloy's; see CLAUDE.md, Projects. Code
//! here names no `rquickjs` / `'js` types; a pure-Rust host could use it
//! directly. The scripting host (flux) depends on this crate and supplies
//! all the marshalling.

#[cfg(feature = "audio")]
pub mod audio;
pub mod cache;
pub mod compression;
pub mod crypto;
pub mod events;
pub mod fetch;
pub mod fs;
pub mod http;
pub mod image;
pub mod isolate;
#[cfg(feature = "ktx2")]
pub mod ktx2;
pub mod logger;
pub mod mdns;
pub mod net;
pub mod p2p;
pub mod path;
pub mod process;
pub mod seek;
pub mod source;
pub mod sqlite;
pub mod stream;
pub mod subprocess;
pub mod svg;
pub mod trailer;
pub mod tty;
pub mod value;
#[cfg(feature = "video")]
pub mod video;
#[cfg(feature = "wasm")]
pub mod wasm;
pub mod websocket;
pub mod workers;

#[cfg(test)]
mod tests;

pub use seek::{SeekableRead, SeekableReader};
pub use value::{Elem, Value};
