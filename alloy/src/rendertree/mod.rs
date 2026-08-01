pub mod composite;
pub mod counters;
pub mod hit;
pub(crate) mod kinds;
pub mod layout;
pub mod platform;
mod tree;

pub use hit::{HitConfig, PointerEvents};
pub use kinds::{
  fit_rects, Gradient, GradientStop, GradientUnits, Line, OriginCoord, Oval, PaintState, Path, Rectangle, Span,
  Text, Texture, TextureFit, View, Window,
};
pub use layout::{LayoutCache, LayoutContext, LayoutData};
pub use platform::{FontPayload, PlatformContext};
pub use tree::{NodeMatch, NodeSnapshot, RenderTree};

use crate::impellers::{DisplayList, DisplayListBuilder, Texture as ImpellerTexture};
use std::cell::RefCell;
use taffy::{AvailableSpace, Position, Style};

// The rendertree's geometry vocabulary is euclid, spelled through the
// impellers aliases so the types unify with every draw call. taffy geometry
// appears only as layout input (MeasureContext) and is always written
// taffy-qualified. Vector completes the euclid set (impellers does not alias
// it): offsets like translate and scroll, and what point arithmetic yields.
pub use crate::impellers::{Point, Rect, Size};
pub type Vector = euclid::Vector2D<f32, euclid::UnknownUnit>;

/// Build context passed during display list tree traversal. Engine state
/// (platform, alloy) comes first; paint-time geometry follows.
pub struct BuildContext<'a> {
  pub platform: &'a PlatformContext,
  pub alloy: &'a crate::Context,
  pub size: Size,
  // Repaint-boundary diagnostics for the frame being built (see composite.rs).
  pub boundaries_reused: u32,
  pub boundaries_recorded: u32,
  pub snapshots_reused: u32,
  pub snapshots_rerendered: u32,
  pub snapshots_rasterized: u32,
}

impl<'a> BuildContext<'a> {
  pub fn new(platform: &'a PlatformContext, alloy: &'a crate::Context) -> Self {
    Self {
      platform,
      alloy,
      size: Size::default(),
      boundaries_reused: 0,
      boundaries_recorded: 0,
      snapshots_reused: 0,
      snapshots_rerendered: 0,
      snapshots_rasterized: 0,
    }
  }
}

/// Measure context passed during layout. Engine state (platform, alloy) comes
/// first; the taffy-supplied size constraints for this call follow. The
/// constraints stay taffy types on purpose: measure is called BY taffy with
/// taffy's constraint semantics (AvailableSpace has no euclid equivalent);
/// only the answer is euclid.
pub struct MeasureContext<'a> {
  pub platform: &'a PlatformContext,
  pub alloy: &'a crate::Context,
  pub known: taffy::Size<Option<f32>>,
  pub available: taffy::Size<AvailableSpace>,
}

/// Trait for element type build behavior
pub trait Buildable {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder);
}

/// Trait for content-based sizing (text, images, etc.)
pub trait Measurable {
  fn measure(&self, ctx: &MeasureContext) -> Size;
}

/// A kind's painted box relative to its own origin: the rect's origin is the
/// paint offset, its size the painted size. `fallback` supplies the size when
/// the kind carries no explicit `w`/`h`.
pub trait Bounded {
  fn local_bounds(&self, fallback: Size) -> Rect;
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
  /// The kind's canonical name, matching the attached-variant names accepted by
  /// `Element::from_kind`. Whether a node is detached is not part of the kind;
  /// it lives on `Element::layout`.
  pub fn name(&self) -> &'static str {
    match self {
      ElementKind::Window(_) => "window",
      ElementKind::View(_) => "view",
      ElementKind::Rectangle(_) => "rect",
      ElementKind::Oval(_) => "oval",
      ElementKind::Line(_) => "line",
      ElementKind::Path(_) => "path",
      ElementKind::Text(_) => "text",
      ElementKind::Span(_) => "span",
      ElementKind::Texture(_) => "texture",
    }
  }

  pub fn paint_mut(&mut self) -> Option<&mut PaintState> {
    match self {
      ElementKind::Rectangle(r) => Some(&mut r.paint),
      ElementKind::Oval(o) => Some(&mut o.paint),
      ElementKind::Line(l) => Some(&mut l.paint),
      ElementKind::Path(p) => Some(&mut p.paint),
      ElementKind::Text(t) => Some(&mut t.paint),
      ElementKind::Texture(t) => Some(&mut t.paint),
      _ => None,
    }
  }

  /// Kinds sized by their own geometry (leaves), as opposed to container kinds
  /// sized by their children. MUST stay in sync with the arms of `Measurable
  /// for ElementKind` below: a leaf here is a kind that has a real `measure`.
  pub fn is_measured_leaf(&self) -> bool {
    matches!(
      self,
      ElementKind::Text(_)
        | ElementKind::Rectangle(_)
        | ElementKind::Oval(_)
        | ElementKind::Line(_)
        | ElementKind::Path(_)
        | ElementKind::Texture(_)
    )
  }

  /// Dispatches to each kind's `Bounded` impl; kinds without one default to
  /// `fallback`. For Line and Path that is a known approximation, since they
  /// paint in their own coordinate space.
  pub fn local_bounds(&self, fallback: Size) -> Rect {
    match self {
      ElementKind::Rectangle(n) => n.local_bounds(fallback),
      ElementKind::Oval(n) => n.local_bounds(fallback),
      ElementKind::View(n) => n.local_bounds(fallback),
      ElementKind::Text(n) => n.local_bounds(fallback),
      ElementKind::Texture(n) => n.local_bounds(fallback),
      _ => Rect::new(Point::zero(), fallback),
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
  fn measure(&self, ctx: &MeasureContext) -> Size {
    match self {
      ElementKind::Text(n) => n.measure(ctx),
      ElementKind::Texture(n) => n.measure(ctx),
      ElementKind::Path(n) => n.measure(ctx),
      ElementKind::Oval(n) => n.measure(ctx),
      ElementKind::Line(n) => n.measure(ctx),
      ElementKind::Rectangle(n) => n.measure(ctx),
      _ => Size::zero(),
    }
  }
}

/// What a property write invalidates, reported by each setter and consumed by
/// RenderTree::apply_damage. Ordered by scope; every variant implies the ones
/// below it stay valid.
///
/// `Transform` marks a write to a View's composite-time matrix: the node's own
/// cached content stays valid (composite applies the current matrix around it;
/// see composite::hoisted_matrix), but ancestor boundaries hold the node at its
/// old placement and must repaint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Damage {
  /// No visual change (window chrome, hit-testing config).
  None,
  /// Pixels may change without any tree content changing: a new present is
  /// needed but every cache - including the built display list - stays
  /// valid. The window shader's prop writes report this; the present-only
  /// reuse path (lattice renderFrame) resubmits the cached list for it.
  Present,
  /// The node's own transform changed; its content caches survive.
  Transform,
  /// The node's scroll offset changed. A Recording cache survives (clip and
  /// scroll are applied around it at composite time; see composite::Hoist),
  /// but a Snapshot texture does not contain scrolled-out pixels and must
  /// re-rasterize.
  Scroll,
  /// Painted content changed; paint caches clear from the node up.
  Paint,
  /// Layout inputs changed; taffy caches and paint caches clear.
  Layout,
}

/// What a repaint boundary retains across frames: nothing, the recorded
/// display list (skips rebuilding), or rasterized pixels (skips rasterizing
/// too, at the cost of GPU memory and resolution-dependence). SnapshotNoAa
/// is Snapshot rasterized single-sample: no multisampled scratch, no resolve
/// pass, but vector content (svg paths, rotated edges) comes out hard-edged;
/// the app author opts in per boundary.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundaryMode {
  #[default]
  None,
  Recording,
  Snapshot,
  SnapshotNoAa,
}

/// A boundary's retained paint result, in node-local coordinates. A snapshot
/// remembers the logical size and display scale it was rasterized at: pixels
/// are resolution-dependent, so a mismatch forces re-rasterization even when
/// nothing inside the subtree changed. Invalidation marks a snapshot stale
/// (`valid: false`) instead of dropping it: the pixels are worthless but the
/// texture allocation is still exactly the right size, so the next raster
/// re-renders into it instead of rebuilding the whole offscreen rig (see
/// composite::snapshot_node).
pub enum PaintCache {
  Recording(DisplayList),
  Snapshot { texture: ImpellerTexture, width: f32, height: f32, scale: f32, valid: bool },
}

pub struct Element {
  pub kind: ElementKind,
  pub children: Vec<u64>,
  pub parent: Option<u64>,
  pub layout: Option<LayoutData>,
  pub interaction: Option<HitConfig>,
  // Explicit repaint boundary (Flutter's RepaintBoundary / SnapshotWidget):
  // the subtree's paint result is retained while nothing inside changes.
  pub repaint_boundary: BoundaryMode,
  // The boundary's retained paint result. Cleared by
  // RenderTree::invalidate_paint on any content or layout change in the
  // subtree. Interior-mutable because painting traverses a shared tree.
  pub paint_cache: RefCell<Option<PaintCache>>,
}

impl Element {
  pub fn with_layout(kind: ElementKind, style: Style) -> Self {
    Self {
      kind,
      children: vec![],
      parent: None,
      layout: Some(LayoutData::new(style)),
      interaction: Some(HitConfig::default()),
      repaint_boundary: BoundaryMode::None,
      paint_cache: RefCell::new(None),
    }
  }

  pub fn no_layout(kind: ElementKind) -> Self {
    Self {
      kind,
      children: vec![],
      parent: None,
      layout: None,
      interaction: Some(HitConfig::default()),
      repaint_boundary: BoundaryMode::None,
      paint_cache: RefCell::new(None),
    }
  }

  /// Builds an element from its JSX tag name. The root Window is created via
  /// RenderTree::create_root instead, so "window" is rejected here.
  pub fn from_kind(kind: &str) -> Element {
    match kind {
      "window" => panic!("use createRoot to create the root Window node"),
      "view" => View::default().with_layout(),
      "d-view" => View::default().no_layout(),
      "rect" => Rectangle::default().with_layout(),
      "d-rect" => Rectangle::default().no_layout(),
      "oval" => Oval::default().with_layout(),
      "d-oval" => Oval::default().no_layout(),
      "line" => Line::default().with_layout(),
      "d-line" => Line::default().no_layout(),
      "path" => Path::default().with_layout(),
      "d-path" => Path::default().no_layout(),
      "text" => Text::default().with_layout(),
      "d-text" => Text::default().no_layout(),
      "d-span" => Span::default().no_layout(),
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

  /// Sets the taffy position and records whether this node is an explicit
  /// positioning context (only `Relative` counts; see LayoutData). Position has
  /// this side effect beyond the taffy Style, so it routes through here rather
  /// than the direct style adapter.
  pub fn set_position(&mut self, position: Position) {
    let layout = self.layout.as_mut().expect("position requires a layout element");
    layout.style.position = position;
    layout.positioning_context = matches!(position, Position::Relative);
  }

  /// Sets how this element participates in hit testing. Paint/hit only; never
  /// affects layout. `None` clears any local override, so the element goes
  /// back to inheriting its effective value from the nearest ancestor that
  /// sets one (see HitConfig::pointer_events).
  pub fn set_pointer_events(&mut self, pointer_events: Option<PointerEvents>) {
    self.interaction.get_or_insert_with(HitConfig::default).pointer_events = pointer_events;
  }

  pub fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    self.kind.build(ctx, builder);
  }
}
