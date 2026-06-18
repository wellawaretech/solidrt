//! Engine-free capability cores.
//!
//! `forge` holds the scripting-engine-independent foundations that the plugin
//! (marshalling) layer builds on. Code here names no `rquickjs` / `'js` types; a
//! pure-Rust host could use it directly. Today it carries the HTTP server core;
//! more capability cores (the engine-free halves of sqlite, p2p, subprocess, ...)
//! land here as they are split out. Destined to become its own crate (see
//! REDESIGN.md).

pub mod http;
