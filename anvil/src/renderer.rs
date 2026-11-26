//! The renderer and its texture handle, over the recording (`record`, its
//! ops in `ops`, its paths in `path`), the pure walk (`walk`), the path
//! meshes (`mesh`, pure too), the ramp atlas's rows (`ramps`, pure too) and
//! the executor (`exec`); the crate doc says what draws.
//!
//! An adopted GL name belongs to the renderer that adopted it. The handle
//! is `Arc`-shared and crosses threads (a list is recorded on the UI thread
//! and drawn on the raster thread; the context-restored event carries
//! handles to the UI thread), so the last clone may drop on a thread with no
//! GL context current, and deletion needs the raster thread with the
//! context current. So a handle never deletes: its drop pushes the name
//! onto a queue the renderer created, the executor drains that queue at the
//! start of every `draw` and deletes the names there, and a renderer that is
//! dropped (a context loss, alloy's `raster/recover.rs`) takes its queue with it.
//! Handles that outlive it push onto a queue nobody drains, which is right,
//! because those names died with the context; the new renderer drains only
//! the queue it created, so a stale name (possibly the same number as a
//! fresh one on the new context) is never deleted on the new context.
//! Impeller's reactor did this quietly; anvil does it here. A paint
//! shader's program is held the same way, on the same queue, and so are a
//! path mesh's vertex and index buffers (`mesh::GpuMesh`).

use crate::contract;
use crate::{ISize, TextureSampling, TextureWrap, UniformInfo};
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
      log::warn!(concat!("[anvil] Renderer: ", $op, " is not supported yet; ", $effect));
    }
  }};
}

/// Warn once per process about a draw the renderer cannot make sense of
/// (not a missing feature: a degenerate argument), and what it does
/// instead.
macro_rules! warn_once {
  ($($arg:tt)*) => {{
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.load(Ordering::Relaxed) && !WARNED.swap(true, Ordering::Relaxed) {
      log::warn!($($arg)*);
    }
  }};
}

pub(crate) mod blend;
pub(crate) mod blur;
mod exec;
mod layers;
pub(crate) mod mesh;
mod ops;
mod path;
pub mod ramps;
mod record;
pub(crate) mod schedule;
pub mod walk;

pub use record::{
  ColorFilter, ColorSource, DisplayList, DisplayListBuilder, ImageFilter, MaskFilter, Paint, Path, PathBuilder,
};

// The paint shader's fragment source assembly, a pure string function, for
// the tests.
#[cfg(test)]
pub(crate) use exec::{paint_fragment_source, PAINT_FRAGMENT_EPILOGUE, SOURCE_HEADER};

// The composite's texel rows rescaled to a layer's storage, a pure
// function, for the tests.
#[cfg(test)]
pub(crate) use layers::rescale_composite;

// The destination copy's read rect and the composite program's blend
// functions, for the tests.
#[cfg(test)]
pub(crate) use exec::{blit_rect, layer_passes, Frame, COMPOSITE_BLEND};

// The backdrop fill's rects and the color matrix's uniforms, pure
// functions, for the tests.
#[cfg(test)]
pub(crate) use exec::{clamp_rect, fill_rects, matrix_uniforms};

// The shadow program's fragment source and the recording's mask sigma
// floor, for the tests.
#[cfg(test)]
pub(crate) use exec::SHADOW_FRAGMENT;
#[cfg(test)]
pub(crate) use record::MASK_SIGMA_MIN;

/// A GL name whose last handle dropped: a texture's, a paint shader's
/// program, or a path mesh's vertex and index buffers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Freed {
  Texture(glow::NativeTexture),
  Program(glow::NativeProgram),
  Buffers(glow::NativeBuffer, glow::NativeBuffer),
}

/// Names whose last handle dropped, waiting for the raster thread to
/// delete them at the next draw (see the module doc: a handle may drop on
/// any thread, deletion needs the context).
#[derive(Debug)]
pub(crate) struct DeletionQueue(Mutex<Vec<Freed>>);

impl DeletionQueue {
  pub(crate) fn new() -> Self {
    DeletionQueue(Mutex::new(Vec::new()))
  }

  pub(crate) fn push(&self, name: Freed) {
    // A poisoned lock only means a panic elsewhere while it was held; the
    // vector of names is still whole, so the push goes on.
    self.0.lock().unwrap_or_else(|e| e.into_inner()).push(name);
  }

  /// Everything pushed so far, leaving the queue empty.
  pub(crate) fn drain(&self) -> Vec<Freed> {
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

  fn create_paint_shader(&self, fragment_src: &str) -> Result<PaintShader, String> {
    let linked = self.exec.link_paint(fragment_src)?;
    Ok(PaintShader(Arc::new(PaintShaderInner { linked, queue: self.queue.clone() })))
  }

  fn draw(&mut self, target: contract::Target, dl: &DisplayList) -> Result<contract::DrawReport, String> {
    // The drain comes first, so a name freed on the UI thread is reclaimed
    // on the first draw after it, before the draw's own GL work.
    self.exec.reclaim(self.queue.drain());
    self.exec.draw(target, dl, &self.queue)
  }
}

/// What drawing `walked` issues: every instance it wrote, one draw call per
/// batch (`exec` draws each batch once, instanced quads, a mesh's elements
/// or a stencil clip draw; a layer's two markers are one call, its
/// composite, and an empty layer's markers and what lies between them none),
/// the path meshes it drew, how many meshes it tessellated, the stencil clip
/// draws it made, the layers it drew offscreen, those of them drawn ahead
/// of the frame they composite into (`schedule`), and their blur passes, a
/// backdrop's fill copy among them (`blur::blur_passes`). The walk decides
/// all eight, so the tests read the report off a walk with no GL.
pub(crate) fn report(walked: &walk::Walked) -> contract::DrawReport {
  contract::DrawReport {
    instances: walked.instances.len() as u32,
    draw_calls: draw_calls(walked),
    meshes: walked.meshes,
    tessellated: walked.tessellated,
    clips: walked.clip_draws.len() as u32,
    layers: walked.layers.iter().filter(|l| !l.folded && !l.rect.is_empty()).count() as u32,
    layers_ahead: walked.layers.iter().filter(|l| !l.folded && !l.rect.is_empty() && schedule::draws_ahead(l)).count()
      as u32,
    blur_passes: walked
      .layers
      .iter()
      .filter(|l| !l.folded && !l.rect.is_empty())
      .map(|l| {
        let (sx, sy) = l.blur.map_or((0.0, 0.0), |b| (b.sigma_x, b.sigma_y));
        blur::blur_passes(sx, sy, l.backdrop)
      })
      .sum(),
    // The executor's, from its schedule (`exec::layer_passes`): the
    // schedule needs the atlas size, which the walk does not know.
    layer_passes: 0,
  }
}

// The draw calls `walked`'s batches issue, as `exec` skips and draws them:
// one per batch, none for a `LayerBegin` (its `LayerEnd` draws the
// composite), and none from an empty layer's `LayerBegin` through its
// `LayerEnd` (`skip` counts the empty layers open).
fn draw_calls(walked: &walk::Walked) -> u32 {
  use walk::BatchProgram::{LayerBegin, LayerEnd};
  let mut calls = 0;
  let mut skip = 0usize;
  for batch in &walked.batches {
    match batch.program {
      LayerBegin if skip > 0 || walked.layers[batch.start].rect.is_empty() => skip += 1,
      LayerEnd if skip > 0 => skip -= 1,
      _ if skip > 0 => {}
      LayerBegin => {}
      _ => calls += 1,
    }
  }
  calls
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
    self.queue.push(Freed::Texture(self.name));
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
  #[cfg(any(test, feature = "oplog"))]
  pub(crate) fn over(queue: Arc<DeletionQueue>, name: u32, size: ISize) -> Texture {
    let name = glow::NativeTexture(std::num::NonZeroU32::new(name).expect("a GL name is never zero"));
    Texture(Arc::new(TextureInner { name, size, queue }))
  }
}

/// A texture handle with no renderer behind it, for tests that record
/// without GL: a fresh queue nobody drains, so its drop deletes nothing.
#[cfg(any(test, feature = "oplog"))]
pub fn test_texture(name: u32, size: ISize) -> Texture {
  Texture::over(Arc::new(DeletionQueue::new()), name, size)
}

impl contract::Texture for Texture {
  fn gl_name(&self) -> Option<glow::NativeTexture> {
    Some(self.0.name)
  }
}

/// A sampler2D input of a paint shader: the uniform `name` samples
/// `texture` through `sampling` and `wrap`.
#[derive(Clone)]
pub struct ShaderInput {
  pub name: String,
  pub texture: Texture,
  pub sampling: TextureSampling,
  pub wrap: TextureWrap,
}

/// An app's fragment program, linked behind anvil's own vertex stage, that
/// a list draws as a quad (`DisplayListBuilder::draw_shader`). Shared by
/// handle like a texture: the program is the renderer's, and the last
/// handle's drop pushes it onto the creating renderer's deletion queue,
/// never deletes it itself (the module doc says why). Two handles are equal
/// when they are clones of one.
#[derive(Clone)]
pub struct PaintShader(Arc<PaintShaderInner>);

struct PaintShaderInner {
  linked: exec::PaintProgram,
  queue: Arc<DeletionQueue>,
}

impl Drop for PaintShaderInner {
  fn drop(&mut self) {
    self.queue.push(Freed::Program(self.linked.program));
  }
}

impl PaintShader {
  fn linked(&self) -> &exec::PaintProgram {
    &self.0.linked
  }

  /// A handle over `queue` with the program name `name` and no uniforms
  /// (test builds build one without a renderer).
  #[cfg(any(test, feature = "oplog"))]
  pub(crate) fn over(queue: Arc<DeletionQueue>, name: u32) -> PaintShader {
    let program = glow::NativeProgram(std::num::NonZeroU32::new(name).expect("a GL name is never zero"));
    let linked = exec::PaintProgram::without_gl(program);
    PaintShader(Arc::new(PaintShaderInner { linked, queue }))
  }
}

impl PartialEq for PaintShader {
  fn eq(&self, other: &PaintShader) -> bool {
    Arc::ptr_eq(&self.0, &other.0)
  }
}

impl std::fmt::Debug for PaintShader {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("PaintShader").field(&self.0.linked.program).finish()
  }
}

/// A paint shader handle with no renderer behind it, for tests that record
/// and walk without GL: a dummy program name, no uniforms, and a fresh
/// queue nobody drains, so its drop deletes nothing.
#[cfg(any(test, feature = "oplog"))]
pub fn test_paint_shader() -> PaintShader {
  PaintShader::over(Arc::new(DeletionQueue::new()), TEST_PROGRAM_NAME)
}

/// The program name `test_paint_shader` carries: any non-zero name, as no
/// GL ever sees it.
#[cfg(any(test, feature = "oplog"))]
const TEST_PROGRAM_NAME: u32 = 1;

impl contract::PaintShader for PaintShader {
  fn uniforms(&self) -> &[UniformInfo] {
    &self.0.linked.infos
  }
}
