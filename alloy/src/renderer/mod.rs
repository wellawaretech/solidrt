//! The renderer seam. Alloy describes a frame in its own vocabulary
//! (`vocab`) through the recording types, and draws it through one renderer
//! that implements the contract (`contract`); which renderer is a
//! compile-time choice, and a build holds exactly one. That is anvil
//! (`anvil`), alloy's own (okf/plans/own-renderer.md); Impeller and its
//! crate are gone. The aliases below are the one place that names the
//! renderer's types, so nothing outside this module changes with it.
//!
//! In a test build the recording types also keep a plain-data log of what
//! they recorded (`oplog`), which a list returns through
//! `DisplayList::ops`, so a test reads back what the paint walk drew.
//!
//! The aliases are the names the rest of the code uses; the traits are in
//! scope through the prelude:
//!
//! ```ignore
//! use crate::renderer::prelude::*;
//! use crate::renderer::{DisplayListBuilder, Paint, Rect};
//! ```

mod anvil;
pub mod contract;
#[cfg(test)]
pub mod oplog;
mod vocab;

pub use contract::Target;
pub use vocab::*;

pub type Renderer = anvil::Renderer;
pub type Texture = anvil::Texture;
pub type DisplayList = anvil::DisplayList;
pub type DisplayListBuilder = anvil::DisplayListBuilder;
pub type Paint = anvil::Paint;
pub type ColorSource = anvil::ColorSource;
pub type ColorFilter = anvil::ColorFilter;
pub type ImageFilter = anvil::ImageFilter;
pub type MaskFilter = anvil::MaskFilter;
pub type Path = anvil::Path;
pub type PathBuilder = anvil::PathBuilder;

/// The contract traits, anonymously: glob-import this to call the
/// methods on the aliases above without naming the traits beside them.
pub mod prelude {
  pub use super::contract::{
    ColorFilter as _, ColorSource as _, DisplayList as _, DisplayListBuilder as _, ImageFilter as _, MaskFilter as _,
    Paint as _, Path as _, PathBuilder as _, Pen as _, Renderer as _, Texture as _,
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
}
