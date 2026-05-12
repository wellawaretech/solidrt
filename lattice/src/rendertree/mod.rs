pub mod composite;
pub mod hit;
mod kinds;
pub mod layout;
pub mod platform;
mod tree;

pub use hit::{HitConfig, HitTester};
pub use kinds::{PaintState, Rectangle, Span, Text, View, Window};
pub use layout::{LayoutContext, LayoutData};
pub use platform::PlatformContext;
pub use tree::RenderTree;

use alloy::impellers::DisplayListBuilder;
use taffy::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct XY {
  pub x: f32,
  pub y: f32,
}

impl XY {
  pub fn new(x: f32, y: f32) -> Self {
    Self { x, y }
  }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WH {
  pub w: f32,
  pub h: f32,
}

impl WH {
  pub fn new(w: f32, h: f32) -> Self {
    Self { w, h }
  }
}

/// Build context passed during display list tree traversal.
pub struct BuildContext<'a> {
  pub platform: &'a PlatformContext,
  pub size: WH,
  pub origin: XY,
}

impl<'a> BuildContext<'a> {
  pub fn new(platform: &'a PlatformContext) -> Self {
    Self {
      platform,
      size: WH::default(),
      origin: XY::default(),
    }
  }
}

/// Trait for element type build behavior
pub trait Buildable {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder);
}

/// Trait for content-based sizing (text, images, etc.)
pub trait Measurable {
  fn measure(
    &self,
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    platform: &PlatformContext,
  ) -> Size<f32>;
}

pub enum ElementKind {
  Window(Window),
  View(View),
  Rectangle(Rectangle),
  // Oval(Oval),
  // Path(Path),
  Text(Text),
  Span(Span),
  // Texture(Texture),
  // Audio(Audio),
}

impl ElementKind {
  pub fn paint_mut(&mut self) -> Option<&mut PaintState> {
    match self {
      ElementKind::Rectangle(r) => Some(&mut r.paint),
      ElementKind::Text(t) => Some(&mut t.paint),
      _ => None,
    }
  }
}

impl Buildable for ElementKind {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    match self {
      ElementKind::Window(n) => n.build(ctx, builder),
      ElementKind::View(n) => n.build(ctx, builder),
      ElementKind::Rectangle(n) => n.build(ctx, builder),
      // ElementKind::Oval(n) => n.build(ctx, builder),
      // ElementKind::Path(n) => n.build(ctx, builder),
      ElementKind::Text(n) => n.build(ctx, builder),
      // ElementKind::Texture(n) => n.build(ctx, builder),
      ElementKind::Span(_) => {} // ElementKind::Audio(_) => {}
    }
  }
}

impl Measurable for ElementKind {
  fn measure(
    &self,
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    platform: &PlatformContext,
  ) -> Size<f32> {
    match self {
      ElementKind::Text(n) => n.measure(known_dimensions, available_space, platform),
      // ElementKind::Texture(n) => n.measure(known_dimensions, available_space, platform),
      // ElementKind::Path(n) => n.measure(known_dimensions, available_space, platform),
      // ElementKind::Oval(n) => n.measure(known_dimensions, available_space, platform),
      ElementKind::Rectangle(n) => n.measure(known_dimensions, available_space, platform),
      _ => Size::ZERO,
    }
  }
}

pub struct Element {
  pub kind: ElementKind,
  pub children: Vec<u64>,
  pub parent: Option<u64>,
  pub layout: Option<LayoutData>,
  pub interaction: Option<HitConfig>,
}

impl Element {
  pub fn with_layout(kind: ElementKind, style: Style) -> Self {
    Self {
      kind,
      children: vec![],
      parent: None,
      layout: Some(LayoutData::new(style)),
      interaction: None,
    }
  }

  pub fn no_layout(kind: ElementKind) -> Self {
    Self {
      kind,
      children: vec![],
      parent: None,
      layout: None,
      interaction: None,
    }
  }

  pub fn has_layout(&self) -> bool {
    self.layout.is_some()
  }

  pub fn layout_data(&self) -> &LayoutData {
    self.layout.as_ref().expect("element has no layout data")
  }

  pub fn layout_data_mut(&mut self) -> &mut LayoutData {
    self.layout.as_mut().expect("element has no layout data")
  }

  pub fn style_mut(&mut self) -> Option<&mut Style> {
    self.layout.as_mut().map(|l| &mut l.style)
  }

  pub fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    self.kind.build(ctx, builder);
  }
}
