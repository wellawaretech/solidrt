//! Anvil, the 2D renderer behind alloy's renderer seam
//! (okf/plans/own-renderer.md). Alloy describes a frame in this crate's
//! vocabulary (`vocab`, and the glyph vocabulary of `glyphs`) through the
//! recording types, and draws it through the renderer, which implements the
//! contract (`contract`). Which renderer is a compile-time choice, and a
//! build holds exactly one; Impeller and its crate are gone. Alloy's
//! `src/renderer/` re-exports this crate and is the one place in alloy that
//! names it, so nothing else in alloy changes with the renderer.
//!
//! A builder records anvil's own executable ops (`ops`, recorded by
//! `record`, beside the op log `oplog`); a draw walks them into instances
//! (`walk`, pure) and draws those as instanced quads through three programs
//! (`exec`): plain rect fills through a fill program with no discard and no
//! derivatives, every other shape through the shape program, glyph runs
//! through a glyph program of their own. A paint shader (an app's fragment
//! program, linked by `Renderer::create_paint_shader` behind anvil's own
//! vertex stage) draws as one quad through its own program, a batch of its
//! own per draw. Stage 1 draws the state ops, rect
//! and oval fills and strokes, lines, texture quads and the axis-aligned
//! intersect clip. Stage 2, step 1 adds rounded rects (per-corner radii,
//! fill, stroke and stroke-and-fill), an axis-aligned rounded clip, and
//! rects, rounded rects and ovals filled with an image color source (the
//! rounded `<texture>`). A glyph run draws straight from the text atlas:
//! one instance per glyph, the atlas's red channel as coverage under the
//! run's coverage policy, a gradient run from a ramp, so a screen of text
//! is a few batches over one texture: the glyph runs, and the shapes
//! between them in batches of their own. What does not draw yet (paths,
//! layers, the other clips, group opacity, gradients, filters, blend modes)
//! warns once per process, naming itself and what happens instead.
//!
//! With the `oplog` feature (alloy enables it for its tests only), and in
//! this crate's own tests, the recording types also keep a plain-data log
//! of what they recorded (`oplog`), which a list returns through
//! `DisplayList::ops`, so a test reads back what the paint walk drew. A
//! shipped build has no log.
//!
//! The concrete types are re-exported here under the contract's names; the
//! traits are in scope through the prelude:
//!
//! ```ignore
//! use anvil::prelude::*;
//! use anvil::{DisplayListBuilder, Paint, Rect};
//! ```

pub mod contract;
mod glyphs;
#[cfg(any(test, feature = "oplog"))]
pub mod oplog;
mod renderer;
mod vocab;

pub use contract::Target;
pub use glyphs::{CoverageMode, CoveragePolicy, GlyphGradient, GlyphQuad, GradientKind, RAMP_WIDTH};
pub use renderer::{
  ColorFilter, ColorSource, DisplayList, DisplayListBuilder, ImageFilter, MaskFilter, Paint, PaintShader, Path,
  PathBuilder, Renderer, ShaderInput, Texture,
};
pub use vocab::*;

// A texture handle with no renderer behind it, for tests that record
// without GL.
#[cfg(any(test, feature = "oplog"))]
pub use renderer::test_texture;
// A paint shader handle with no program behind it, for the same tests.
#[cfg(any(test, feature = "oplog"))]
pub use renderer::test_paint_shader;
// The pure walk, for tests that read which instances and batches a list
// draws.
#[cfg(feature = "oplog")]
pub use renderer::walk;

/// The contract traits, anonymously: glob-import this to call the
/// methods on the concrete types without naming the traits beside them.
pub mod prelude {
  pub use super::contract::{
    ColorFilter as _, ColorSource as _, DisplayList as _, DisplayListBuilder as _, ImageFilter as _, MaskFilter as _,
    Paint as _, PaintShader as _, Path as _, PathBuilder as _, Pen as _, Renderer as _, Texture as _,
  };
}

// The selected implementation satisfies the whole contract: a renderer
// that falls short fails here, not at its first caller.
#[allow(dead_code)]
fn selected_renderer_implements_the_contract() {
  fn renderer<T: contract::Renderer>() {}
  fn texture<T: contract::Texture>() {}
  fn display_list<T: contract::DisplayList>() {}
  fn builder<T: contract::DisplayListBuilder>() {}
  fn paint<T: contract::Paint>() {}
  fn color_source<T: contract::ColorSource>() {}
  fn color_filter<T: contract::ColorFilter>() {}
  fn image_filter<T: contract::ImageFilter>() {}
  fn mask_filter<T: contract::MaskFilter>() {}
  fn path<T: contract::Path>() {}
  fn pen<T: contract::Pen>() {}
  fn path_builder<T: contract::PathBuilder>() {}
  fn paint_shader<T: contract::PaintShader>() {}
  renderer::<Renderer>();
  texture::<Texture>();
  display_list::<DisplayList>();
  builder::<DisplayListBuilder>();
  paint::<Paint>();
  color_source::<ColorSource>();
  color_filter::<ColorFilter>();
  image_filter::<ImageFilter>();
  mask_filter::<MaskFilter>();
  path::<Path>();
  pen::<PathBuilder>();
  path_builder::<PathBuilder>();
  paint_shader::<PaintShader>();
}

#[cfg(test)]
mod tests;
