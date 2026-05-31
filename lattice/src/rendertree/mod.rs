pub mod composite;
pub mod hit;
mod kinds;
pub mod layout;
pub mod platform;
mod tree;

pub use hit::{HitConfig, HitTester};
pub use kinds::{Line, Oval, PaintState, Path, Rectangle, Span, Text, Texture, View, Window};
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

/// Window-relative axis-aligned bounding box of a node, as returned by
/// RenderTree::bounding_box.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoundingBox {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
}

/// Build context passed during display list tree traversal. Engine state
/// (platform, alloy) comes first; paint-time geometry follows.
pub struct BuildContext<'a> {
  pub platform: &'a PlatformContext,
  pub alloy: &'a alloy::Context,
  pub size: WH,
}

impl<'a> BuildContext<'a> {
  pub fn new(platform: &'a PlatformContext, alloy: &'a alloy::Context) -> Self {
    Self {
      platform,
      alloy,
      size: WH::default(),
    }
  }
}

/// Measure context passed during layout. Engine state (platform, alloy) comes
/// first; the taffy-supplied size constraints for this call follow.
pub struct MeasureContext<'a> {
  pub platform: &'a PlatformContext,
  pub alloy: &'a alloy::Context,
  pub known: Size<Option<f32>>,
  pub available: Size<AvailableSpace>,
}

/// Trait for element type build behavior
pub trait Buildable {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder);
}

/// Trait for content-based sizing (text, images, etc.)
pub trait Measurable {
  fn measure(&self, ctx: &MeasureContext) -> Size<f32>;
}

pub enum ElementKind {
  Window(Window),
  View(View),
  Rectangle(Rectangle),
  Oval(Oval),
  Line(Line),
  Path(Path),
  Text(Text),
  Span(Span),
  Texture(Texture),
  // Audio(Audio),
}

impl ElementKind {
  pub fn paint_mut(&mut self) -> Option<&mut PaintState> {
    match self {
      ElementKind::Rectangle(r) => Some(&mut r.paint),
      ElementKind::Oval(o) => Some(&mut o.paint),
      ElementKind::Line(l) => Some(&mut l.paint),
      ElementKind::Path(p) => Some(&mut p.paint),
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
      ElementKind::Oval(n) => n.build(ctx, builder),
      ElementKind::Line(n) => n.build(ctx, builder),
      ElementKind::Path(n) => n.build(ctx, builder),
      ElementKind::Text(n) => n.build(ctx, builder),
      ElementKind::Texture(n) => n.build(ctx, builder),
      ElementKind::Span(_) => {} // ElementKind::Audio(_) => {}
    }
  }
}

impl Measurable for ElementKind {
  fn measure(&self, ctx: &MeasureContext) -> Size<f32> {
    match self {
      ElementKind::Text(n) => n.measure(ctx),
      ElementKind::Texture(n) => n.measure(ctx),
      ElementKind::Path(n) => n.measure(ctx),
      ElementKind::Oval(n) => n.measure(ctx),
      ElementKind::Line(n) => n.measure(ctx),
      ElementKind::Rectangle(n) => n.measure(ctx),
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
      interaction: Some(HitConfig::default()),
    }
  }

  pub fn no_layout(kind: ElementKind) -> Self {
    Self {
      kind,
      children: vec![],
      parent: None,
      layout: None,
      interaction: Some(HitConfig::default()),
    }
  }

  /// Builds an element from its JSX tag name. The root Window is created via
  /// RenderTree::create_root instead, so "window" is rejected here.
  pub fn from_kind(kind: &str) -> Element {
    match kind {
      "window" => panic!("use createRoot to create the root Window node"),
      "view" => View::default().with_layout(),
      "rect" => Rectangle::default().with_layout(),
      "d-rect" => Rectangle::default().no_layout(),
      "oval" => Oval::default().with_layout(),
      "d-oval" => Oval::default().no_layout(),
      "line" => Line::default().with_layout(),
      "d-line" => Line::default().no_layout(),
      "path" => Path::default().with_layout(),
      "d-path" => Path::default().no_layout(),
      "text" => Text::default().with_layout(),
      "span" => Span::default().no_layout(),
      "texture" => Texture::default().with_layout(),
      "d-texture" => Texture::default().no_layout(),
      _ => panic!("unknown node kind: {kind}"),
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
