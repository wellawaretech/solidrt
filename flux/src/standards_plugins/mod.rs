// Web-standard JS APIs (WHATWG / web platform): console, fetch, the Fetch
// types (Headers / Request / Response / Body), TextEncoder/Decoder, timers, the
// WebSocket client, AbortController/AbortSignal. Installed as globals. The
// Fetch types are reused by the `flux:http` server in the sibling `modules`
// layer.

pub mod abort;
pub mod body;
// Web standard by surface, alloy-backed and installed from `gui::install`
// (see the placement rule in flux/CLAUDE.md), so gui-gated.
#[cfg(feature = "gui")]
pub mod clipboard;
pub mod console;
pub mod crypto;
pub mod fetch;
pub mod headers;
pub mod http;
pub mod request;
pub mod response;
pub mod text;
pub mod time;
pub mod websocket;
