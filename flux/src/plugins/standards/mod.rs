// Web-standard JS APIs (WHATWG / web platform): console, fetch, the Fetch
// types (Headers / Request / Response / Body), TextEncoder/Decoder, timers, the
// WebSocket client. Installed as globals. The Fetch types are reused by the
// `flux:http` server in the sibling `modules` layer.

pub mod base64;
pub mod body;
pub mod console;
pub mod fetch;
pub mod headers;
pub mod http;
pub mod request;
pub mod response;
pub mod text;
pub mod time;
pub mod websocket;
