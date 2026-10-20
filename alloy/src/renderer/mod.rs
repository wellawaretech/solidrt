//! The renderer seam. Alloy describes a frame in its own vocabulary
//! (`vocab`) through the recording types, and draws it through one renderer
//! that implements the contract (`contract`); which renderer is a
//! compile-time choice, and a build holds exactly one. Today that is
//! Impeller (`impeller`), the only module in the workspace that names the
//! `impellers` crate. A replacement is a second module beside it that
//! implements the same traits; the aliases below switch under a cfg while
//! it reaches parity (okf/plans/renderer-seam.md), and nothing outside this
//! module changes.
//!
//! The aliases are the names the rest of the code uses; the traits are in
//! scope through the prelude:
//!
//! ```ignore
//! use crate::renderer::prelude::*;
//! use crate::renderer::{DisplayListBuilder, Paint, Rect};
//! ```

pub mod contract;
mod impeller;
mod vocab;

pub use contract::Target;
pub use vocab::*;

pub type Renderer = impeller::Renderer;
pub type Texture = impeller::Texture;
pub type DisplayList = impeller::DisplayList;
pub type DisplayListBuilder = impeller::DisplayListBuilder;
pub type Paint = impeller::Paint;
pub type ColorSource = impeller::ColorSource;
pub type ColorFilter = impeller::ColorFilter;
pub type ImageFilter = impeller::ImageFilter;
pub type MaskFilter = impeller::MaskFilter;
pub type Path = impeller::Path;
pub type PathBuilder = impeller::PathBuilder;

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
