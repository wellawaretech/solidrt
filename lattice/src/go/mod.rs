#[cfg(target_os = "android")]
mod android;
mod connection;
mod control;
mod proxy;

pub use connection::{start, ConnState, DevServerCell};
pub use control::install_devserver_control;
pub use proxy::{install_proxy_state, ProxyFsModule};