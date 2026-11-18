//! Anvil, alloy's own renderer behind the contract
//! (okf/plans/own-renderer.md). A build holds exactly one renderer, and
//! this is it: Impeller and its crate are gone from the workspace.
//!
//! A builder records anvil's own executable ops (`ops`, recorded by
//! `record`, beside the test-build log `super::oplog`); a draw walks them
//! into instances (`walk`, pure) and draws those as instanced quads through
//! one shape program (`exec`). Stage 1, step 2 draws the state ops, rect
//! fills and the axis-aligned intersect clip; a rounded rect draws as its
//! plain rect. What does not draw yet (strokes, ovals, lines, textures,
//! paths, layers, the other clips, group opacity, color sources, filters,
//! blend modes) warns once per process, naming itself and what happens
//! instead.

use super::contract;
use super::ISize;
use std::os::raw::c_void;
use std::sync::Arc;

/// Warn once per process that an op is not supported yet, and what the
/// renderer does instead; the content degrades, it does not break.
macro_rules! unsupported {
  ($op:literal, $effect:literal) => {{
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    // A plain load once warned: the macro stays on ops recorded per frame.
    if !WARNED.load(Ordering::Relaxed) && !WARNED.swap(true, Ordering::Relaxed) {
      log::warn!(concat!("[alloy] Renderer: ", $op, " is not supported yet; ", $effect));
    }
  }};
}

mod exec;
mod ops;
mod record;
pub(crate) mod walk;

pub use record::{
  ColorFilter, ColorSource, DisplayList, DisplayListBuilder, ImageFilter, MaskFilter, Paint, Path, PathBuilder,
};

/// The drawing half: the executor over the raster thread's GL context.
pub struct Renderer {
  exec: exec::Executor,
}

impl contract::Renderer for Renderer {
  unsafe fn new(proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String> {
    // A second set of function pointers into the same GL context.
    Ok(Renderer { exec: unsafe { exec::Executor::new(proc_address) }? })
  }

  fn adopt_texture(&self, texture: glow::NativeTexture, size: ISize) -> Option<Texture> {
    // Every name is adopted, whatever its format: the callers decide by
    // format whether to adopt at all (raster/targets.rs).
    Some(Texture(Arc::new(TextureInner { name: texture, size })))
  }

  fn draw(&mut self, target: contract::Target, dl: &DisplayList) -> Result<(), String> {
    self.exec.draw(target, dl)
  }
}

/// A GL texture the lists draw: the name and its size, shared by handle.
/// The handle does not delete the name yet: step 3 adds the deferred
/// deletion queue (the last clone may drop on the UI thread, where no GL
/// context is current), so in this step an adopted name is leaked when its
/// last handle drops. Every caller already treats adopted names as the
/// renderer's and never deletes them itself.
#[derive(Clone)]
pub struct Texture(Arc<TextureInner>);

struct TextureInner {
  name: glow::NativeTexture,
  #[allow(dead_code)] // read by the executor from step 3
  size: ISize,
}

impl contract::Texture for Texture {
  fn gl_name(&self) -> Option<glow::NativeTexture> {
    Some(self.0.name)
  }
}
