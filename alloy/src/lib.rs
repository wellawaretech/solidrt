mod gl;
pub mod sdl_utils;

mod app;
mod backend;
mod context;
mod event;
mod logging;
mod texture;

pub use impellers;
pub use sdl3;

pub use app::{setup, App, FrameInfo, RenderHooks};
pub use backend::{create_render_surface, Backend, DisplayContext, RenderSurface};
pub use context::Context;
pub use event::{AlloyCommand, AlloyEvent, Modifiers, PointerType};
pub use logging::install_logger;
pub use texture::{GpuTexture, TextureEntry, TextureRegistry};