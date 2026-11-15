//! The `flux:*` modules that marshal ore, the OS layer: the platform's own
//! facilities, such as opening a link or playing haptic feedback. A web
//! standard that ore backs
//! (`navigator.languages`) is marshalled in `standards_plugins/` instead.
//! The file dialogs need the window as their parent and the pump for their
//! answer, so `flux:dialog` sits behind the `gui` feature. Whether each
//! facility is there now (`Flux.availability`) is marshalled in
//! `availability`, on every build.

pub mod availability;
#[cfg(feature = "gui")]
pub mod dialog;
pub mod haptics;
pub mod link;
