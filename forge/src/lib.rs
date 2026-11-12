//! Capability cores for a headless process.
//!
//! `forge` holds the portable capabilities, the same code on every
//! platform: the HTTP server core, the engine-free halves of sqlite/p2p/
//! subprocess/fetch/fs/path/process/events/websocket, the decoders, the
//! shared byte-stream primitive, and the logging sink. What reaches the OS
//! or SDL (anything per platform, any `cfg(target_os)`) is ore's, the OS
//! layer; forge calls ore where a capability touches the OS. The OS code
//! forge still holds (AMediaCodec in `video`, the content providers in
//! `fs`, the terminal in `tty`, reaching the OS directly, on Android through
//! the JavaVM and activity ore publishes in ndk-context at startup) moves
//! to ore when a feature touches it; new OS code does not land here. See
//! CLAUDE.md, Projects, and okf/design/platform-layering.md. Code here
//! names no `rquickjs` / `'js` types; a pure-Rust host could use it
//! directly. The scripting host (flux) depends on this crate and supplies
//! all the marshalling.

#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "http")]
pub mod cache;
pub mod compression;
pub mod crypto;
pub mod events;
#[cfg(feature = "http")]
pub mod fetch;
pub mod fs;
#[cfg(feature = "http")]
pub mod http;
pub mod image;
pub mod isolate;
#[cfg(feature = "ktx2")]
pub mod ktx2;
pub mod logger;
pub mod mdns;
pub mod net;
#[cfg(any(feature = "p2p", feature = "p2p-relay"))]
pub mod p2p;
pub mod path;
pub mod process;
pub mod seek;
pub mod source;
pub mod sqlite;
pub mod stream;
pub mod subprocess;
pub mod svg;
#[cfg(feature = "tls")]
pub mod tls;
pub mod trailer;
pub mod tty;
pub mod value;
#[cfg(feature = "video")]
pub mod video;
#[cfg(feature = "wasm")]
pub mod wasm;
#[cfg(feature = "http")]
pub mod websocket;
pub mod workers;

#[cfg(test)]
mod tests;

pub use seek::{SeekableRead, SeekableReader};
pub use value::{Elem, Value};
