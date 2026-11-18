//! Anvil, alloy's own renderer behind the contract
//! (okf/plans/own-renderer.md). A build holds exactly one renderer, and
//! this is it: Impeller and its crate are gone from the workspace.
//!
//! A builder records anvil's own executable ops (`ops`, recorded by
//! `record`, beside the test-build log `super::oplog`); a draw walks them
//! into instances (`walk`, pure) and draws those as instanced quads through
//! one shape program (`exec`). Stage 1 draws the state ops, rect and oval
//! fills and strokes, lines, texture quads and the axis-aligned intersect
//! clip. Stage 2, step 1 adds rounded rects (per-corner radii, fill,
//! stroke and stroke-and-fill), an axis-aligned rounded clip, and rects,
//! rounded rects and ovals filled with an image color source (the rounded
//! `<texture>`). A glyph run draws straight from the text atlas: one
//! instance per glyph, the atlas's red channel as coverage under the run's
//! coverage policy, a gradient run from a ramp, so a screen of text and the
//! shapes between it share one batch over one texture. What does not draw
//! yet (paths, layers, the other clips,
//! group opacity, gradients, filters, blend modes) warns once per process,
//! naming itself and what happens instead.
//!
//! An adopted GL name belongs to the renderer that adopted it. The handle
//! is `Arc`-shared and crosses threads (a list is recorded on the UI thread
//! and drawn on the raster thread; the context-restored event carries
//! handles to the UI thread), so the last clone may drop on a thread with no
//! GL context current, and deletion needs the raster thread with the
//! context current. So a handle never deletes: its drop pushes the name
//! onto a queue the renderer created, the executor drains that queue at the
//! start of every `draw` and deletes the names there, and a renderer that is
//! dropped (a context loss, `raster/recover.rs`) takes its queue with it.
//! Handles that outlive it push onto a queue nobody drains, which is right,
//! because those names died with the context; the new renderer drains only
//! the queue it created, so a stale name (possibly the same number as a
//! fresh one on the new context) is never deleted on the new context.
//! Impeller's reactor did this quietly; anvil does it here.

use super::contract;
use super::ISize;
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

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

/// Names whose last handle dropped, waiting for the raster thread to
/// delete them at the next draw (see the module doc: a handle may drop on
/// any thread, deletion needs the context).
pub(crate) struct DeletionQueue(Mutex<Vec<glow::NativeTexture>>);

impl DeletionQueue {
  pub(crate) fn new() -> Self {
    DeletionQueue(Mutex::new(Vec::new()))
  }

  pub(crate) fn push(&self, name: glow::NativeTexture) {
    // A poisoned lock only means a panic elsewhere while it was held; the
    // vector of names is still whole, so the push goes on.
    self.0.lock().unwrap_or_else(|e| e.into_inner()).push(name);
  }

  /// Everything pushed so far, leaving the queue empty.
  pub(crate) fn drain(&self) -> Vec<glow::NativeTexture> {
    // As in `push`: a poisoned lock still guards a whole vector.
    std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
  }
}

/// The drawing half: the executor over the raster thread's GL context.
pub struct Renderer {
  exec: exec::Executor,
  /// The queue this renderer's handles push onto; drained by `draw`.
  queue: Arc<DeletionQueue>,
}

impl contract::Renderer for Renderer {
  unsafe fn new(proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String> {
    // A second set of function pointers into the same GL context.
    Ok(Renderer { exec: unsafe { exec::Executor::new(proc_address) }?, queue: Arc::new(DeletionQueue::new()) })
  }

  fn adopt_texture(&self, texture: glow::NativeTexture, size: ISize) -> Option<Texture> {
    // Every name is adopted, whatever its format: the callers decide by
    // format whether to adopt at all (raster/targets.rs).
    Some(Texture(Arc::new(TextureInner { name: texture, size, queue: self.queue.clone() })))
  }

  fn draw(&mut self, target: contract::Target, dl: &DisplayList) -> Result<(), String> {
    // The drain comes first, so a name freed on the UI thread is reclaimed
    // on the first draw after it, before the draw's own GL work.
    self.exec.reclaim(self.queue.drain());
    self.exec.draw(target, dl)
  }
}

/// A GL texture the lists draw: the name and its size, shared by handle.
/// The name is the renderer's from adoption on: the last handle's drop
/// pushes it onto the adopting renderer's deletion queue, never deletes it
/// itself (the module doc says why), and every caller treats adopted names
/// as the renderer's and never deletes them either.
#[derive(Clone)]
pub struct Texture(Arc<TextureInner>);

struct TextureInner {
  name: glow::NativeTexture,
  size: ISize,
  queue: Arc<DeletionQueue>,
}

impl Drop for TextureInner {
  fn drop(&mut self) {
    self.queue.push(self.name);
  }
}

impl Texture {
  pub(super) fn name(&self) -> glow::NativeTexture {
    self.0.name
  }

  pub(super) fn size(&self) -> ISize {
    self.0.size
  }

  /// A handle over `queue` (test builds build one without a renderer).
  #[cfg(test)]
  pub(crate) fn over(queue: Arc<DeletionQueue>, name: u32, size: ISize) -> Texture {
    let name = glow::NativeTexture(std::num::NonZeroU32::new(name).expect("a GL name is never zero"));
    Texture(Arc::new(TextureInner { name, size, queue }))
  }
}

/// A handle for the walk tests: a fresh queue nobody drains.
#[cfg(test)]
pub(crate) fn test_texture(name: u32, size: ISize) -> Texture {
  Texture::over(Arc::new(DeletionQueue::new()), name, size)
}

impl contract::Texture for Texture {
  fn gl_name(&self) -> Option<glow::NativeTexture> {
    Some(self.0.name)
  }
}
