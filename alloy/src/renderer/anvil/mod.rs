//! Anvil, alloy's own renderer behind the contract
//! (okf/plans/own-renderer.md). A build holds exactly one renderer, and
//! this is it: Impeller and its crate are gone from the workspace.
//!
//! This is the stage 1 stub. Every op records the test-build log
//! (`super::oplog`) and warns once per process that it is not supported
//! yet, naming itself and what happens instead; nothing is drawn. The
//! recording types live in `record`; later steps add the executor that
//! draws what they record.

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

mod record;

pub use record::{
  ColorFilter, ColorSource, DisplayList, DisplayListBuilder, ImageFilter, MaskFilter, Paint, Path, PathBuilder,
};

/// The drawing half. Holds nothing yet: step 2 gives it a glow context
/// over `proc_address`, the shape program and the instance buffer.
pub struct Renderer {
  _private: (),
}

impl contract::Renderer for Renderer {
  unsafe fn new(_proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String> {
    // No GL work yet: step 2 builds a glow context over `proc_address`
    // here, a second set of function pointers into the same GL context.
    Ok(Renderer { _private: () })
  }

  fn adopt_texture(&self, texture: glow::NativeTexture, size: ISize) -> Option<Texture> {
    // Every name is adopted, whatever its format: the callers decide by
    // format whether to adopt at all (raster/targets.rs).
    Some(Texture(Arc::new(TextureInner { name: texture, size })))
  }

  fn draw(&mut self, _target: contract::Target, _dl: &DisplayList) -> Result<(), String> {
    // The contract has a draw clear the target first. Every caller clears
    // before it draws today (gl/draw.rs), so the frame is a cleared target
    // either way; the clear becomes the executor's in step 2.
    unsupported!("draw", "nothing is drawn");
    Ok(())
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
