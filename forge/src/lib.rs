//! Engine-free capability cores.
//!
//! `forge` holds the scripting-engine-independent foundations that the host's
//! marshalling layer builds on. Code here names no `rquickjs` / `'js` types; a
//! pure-Rust host could use it directly. It carries the HTTP server core, the
//! engine-free halves of sqlite/p2p/subprocess/fetch/fs/path/process/events/
//! websocket, the shared byte-stream primitive, and the logging sink. The
//! scripting host (flux) depends on this crate and supplies all the marshalling.

pub mod events;
pub mod fetch;
pub mod fs;
pub mod http;
pub mod logger;
pub mod mdns;
pub mod net;
pub mod p2p;
pub mod path;
pub mod process;
pub mod sqlite;
pub mod stream;
pub mod subprocess;
pub mod websocket;