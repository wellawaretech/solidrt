pub mod composite;
pub mod counters;
pub mod cull;
pub mod frame;
pub mod hit;
pub(crate) mod kinds;
pub mod layout;
pub mod platform;
pub mod router;
pub mod text;
mod tree;

pub use frame::{Commit, FrameBuilder, FrameDriver, PendingFrame};
pub use hit::{EventInterest, HitConfig, PointerEvents};
pub use router::{InputEvent, PointerKey, PointerRouter, RoutedKind, RoutedPointer};
pub use kinds::{
  fit_rects, Gradient, GradientStop, GradientUnits, Line, OriginCoord, Oval, PaintState, Path, Rectangle, Texture,
  TextureFit, View, Window,
};
pub use text::{OverflowWrap, RunOverrides, RunStyle, Span, Text, TextOverflow, TextRun, ATOM_CHAR};
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
  /// What can still be seen, in the frame the walk is currently in; None
  /// culls nothing (see cull.rs).
  pub cull: Option<Rect>,
  /// Nodes whose subtree the walk entered this frame (culled ones excluded).
  pub nodes_painted: u32,
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
      cull: None,
      nodes_painted: 0,
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

  /// Read access to the shared paint, for inspection surfaces.
  pub fn paint(&self) -> Option<&PaintState> {
    match self {
      ElementKind::Rectangle(r) => Some(&r.paint),
      ElementKind::Oval(o) => Some(&o.paint),
      ElementKind::Line(l) => Some(&l.paint),
      ElementKind::Path(p) => Some(&p.paint),
      ElementKind::Text(t) => Some(&t.paint),
      ElementKind::Texture(t) => Some(&t.paint),
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
/// `Compose` marks a write to composite-time state - a View's matrix, its
/// group opacity, a boundary shader declaration: the node's own cached
/// content stays valid (composite applies the current state around or over
/// it; see composite::hoisted_matrix and composite::snapshot_node), but
/// ancestor boundaries hold the node's old composited result and must
/// repaint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Damage {
  /// No visual change (window chrome, hit-testing config).
  None,
  /// Pixels may change without any tree content changing: a new present is
  /// needed but every cache - including the built display list - stays
  /// valid. The window shader's prop writes report this; the present-only
  /// reuse path (lattice renderFrame) resubmits the cached list for it.
  Present,
  /// The node's composite-time state changed; its content caches survive.
  Compose,
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

/// A boundary's retained paint result, in node-local coordinates.
pub enum PaintCache {
  Recording(DisplayList),
  Snapshot(SnapshotCache),
}

/// A snapshot boundary's retained rasterization. It remembers the logical
/// size and display scale it was rasterized at: pixels are
/// resolution-dependent, so a mismatch forces re-rasterization even when
/// nothing inside the subtree changed. Invalidation marks it stale
/// (`valid: false`) instead of dropping it: the pixels are worthless but the
/// texture allocation is still exactly the right size, so the next raster
/// re-renders into it instead of reallocating (see composite::snapshot_node).
/// All storage is exact-size; with an unchanged canvas the allocation is
/// reusable across shader declaration changes in either direction.
pub struct SnapshotCache {
  pub texture: ImpellerTexture,
  pub width: f32,
  pub height: f32,
  pub scale: f32,
  pub valid: bool,
  /// The shader half, present while a boundary shader is declared (see
  /// `View::set_shader`); its output is composited in place of `texture`.
  pub shaded: Option<ShadedCache>,
}

/// The boundary shader's cache: the pass output composited in place of the
/// raw snapshot, the outset the canvas was rasterized with (it joins the
/// validity key - a different outset means different storage), and, with
/// `previous` declared, the prior rasterization retained as the pass's
/// `uPrevious` input.
pub struct ShadedCache {
  pub output: ImpellerTexture,
  pub outset: f32,
  pub history: Option<ImpellerTexture>,
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
  // As an inline atom of a `<text>`: out of the flow against this side, an
  // exclusion for the lines it overlaps. Meaningless anywhere else.
  pub float: Option<text::layout::Side>,
  // As an inline atom: start a line below the text's earlier floats on that
  // side (a floated atom goes below them instead of beside).
  pub clear: Option<text::layout::Clear>,
  // The boundary's retained paint result. Cleared by
  // RenderTree::invalidate_paint on any content or layout change in the
  // subtree. Interior-mutable because painting traverses a shared tree.
  pub paint_cache: RefCell<Option<PaintCache>>,
  // The subtree's paint envelope (see cull.rs), cleared alongside paint_cache.
  pub envelope: cull::EnvelopeCache,
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
      float: None,
      clear: None,
      paint_cache: RefCell::new(None),
      envelope: cull::EnvelopeCache::default(),
    }
  }

  pub fn no_layout(mut kind: ElementKind) -> Self {
    // A detached view has no box of its own, so its unset transform origin
    // pivots at its local (0,0) instead of a box center (see
    // `View::resolve_center`). Set here so every construction path agrees.
    if let ElementKind::View(v) = &mut kind {
      v.detached = true;
    }
    Self {
      kind,
      children: vec![],
      parent: None,
      layout: None,
      interaction: Some(HitConfig::default()),
      repaint_boundary: BoundaryMode::None,
      float: None,
      clear: None,
      paint_cache: RefCell::new(None),
      envelope: cull::EnvelopeCache::default(),
    }
  }

  /// Builds an element from its JSX tag name, `None` for a name that is not
  /// an element. The root Window is created via RenderTree::create_root
  /// instead, so "window" is not one either.
  pub fn from_kind(kind: &str) -> Option<Element> {
    Some(match kind {
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
      // "#text" is a string child (a text leaf), the same node kind as a
      // <span> with text and no overrides.
      "span" | "#text" => Span::default().no_layout(),
      "texture" => Texture::default().with_layout(),
      "d-texture" => Texture::default().no_layout(),
      _ => return None,
    })
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

  pub fn style(&self) -> Option<&Style> {
    self.layout.as_ref().map(|l| &l.style)
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

  /// Sets which routed pointer deliveries this element wants (see
  /// HitConfig::listens and router.rs gating). Pure dispatch metadata: never
  /// affects layout, paint, or hit testing.
  pub fn set_event_interest(&mut self, listens: EventInterest) {
    self.interaction.get_or_insert_with(HitConfig::default).listens = listens;
  }

  pub fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    self.kind.build(ctx, builder);
  }
}
