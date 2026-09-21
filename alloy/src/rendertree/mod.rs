mod boundary;
pub mod composite;
pub mod counters;
pub mod cull;
pub(crate) mod damage;
pub mod frame;
pub mod hit;
pub(crate) mod kinds;
pub mod layout;
pub mod platform;
pub mod router;
pub mod text;
pub mod transitions;
mod tree;

pub use boundary::{BakedBackdrops, PaintCache, RecordingCache, ShadedCache, SnapshotCache};
pub use frame::{Commit, FrameBuilder, FrameDriver, PendingFrame};
pub use hit::{EventInterest, HitConfig, PointerEvents};
pub use kinds::{
  fit_rects, FilterState, Gradient, GradientStop, GradientUnits, Line, OriginCoord, Oval, PaintState, Path, Rectangle,
  ShadowState, Texture, TextureFit, View, Window, DEFAULT_STROKE_WIDTH,
};
pub use layout::{LayoutContext, LayoutData};
pub use platform::{FontPayload, PlatformContext};
pub use router::{InputEvent, PointerKey, PointerRouter, RoutedKind, RoutedPointer};
pub use text::{OverflowWrap, RunOverrides, RunStyle, Span, Text, TextAnchor, TextOverflow, TextRun, ATOM_CHAR};
pub use transitions::{
  AnimKind, AnimProp, AnimValue, Curve, Endpoint, Lifecycle, Slide, TransitionConfig, TransitionEntry, TransitionSpec,
};
pub use tree::{NodeMatch, NodeSnapshot, RenderTree, SlideRemaining};

use crate::impellers::DisplayListBuilder;
use std::cell::{Cell, RefCell};
use taffy::{AvailableSpace, Position, Style};

// The rendertree's geometry vocabulary is euclid, spelled through the
// impellers aliases so the types unify with every draw call. taffy geometry
// appears only as layout input (MeasureContext) and is always written
// taffy-qualified. Vector completes the euclid set (impellers does not alias
// it): offsets like translate and scroll, and what point arithmetic yields.
pub use crate::impellers::{Point, Rect, Size};
pub type Vector = euclid::Vector2D<f32, euclid::UnknownUnit>;

/// A backdrop-filter region noted during the paint walk: the panel's box in
/// window space and its blur reach in logical px. None when the walk could
/// not map it (a non-2D transform in the chain) - any damage then repaints
/// the full frame rather than risking a stale panel.
pub type BackdropRegion = Option<(Rect, f32)>;

/// Build context passed during display list tree traversal. Engine state
/// (platform, alloy) comes first; paint-time geometry follows.
pub struct BuildContext<'a> {
  pub platform: &'a PlatformContext,
  pub alloy: &'a crate::Context,
  /// The node's border box (its full layout box; for a detached node, the
  /// inherited frame). Kinds size their default geometry against it, the box
  /// hit testing measures (okf/done/padding-box-divergence.md).
  pub size: Size,
  /// The node's content box in its own frame: the border box inset by
  /// padding and border (LayoutData::content_box). Equal to `size` at origin
  /// zero when the node has no layout. Text sizes and places against this.
  pub content: Rect,
  /// What can still be seen, in the frame the walk is currently in; None
  /// culls nothing (see cull.rs).
  pub cull: Option<Rect>,
  /// The forward transform from the walk's current frame to window space,
  /// for the damage-extent cells; None past a non-2D matrix (see
  /// cull::WindowMap). Never suspended inside boundary recordings.
  pub to_window: cull::WindowMap,
  /// Nodes whose subtree the walk entered this frame (culled ones excluded).
  pub nodes_painted: u32,
  /// Backdrop-filter regions the walk passed, in window space with each
  /// region's blur reach; None for a region a non-2D transform made
  /// unmappable. Stored on the tree after the walk so damage resolves can
  /// widen a damage rect to whole panels (damage::expand_damage_for_backdrops).
  pub backdrop_regions: Vec<BackdropRegion>,
  // Repaint-boundary diagnostics for the frame being built (see composite.rs).
  pub boundaries_reused: u32,
  pub boundaries_recorded: u32,
  pub snapshots_reused: u32,
  pub snapshots_rerendered: u32,
  pub snapshots_rasterized: u32,
  /// Glass panels whose backdrop layer was emitted ahead of a fading
  /// ancestor's opacity group this frame (composite::emit_backdrops_below).
  pub backdrops_prepainted: u32,
  /// Set while a backdrops-only pass runs (composite::emit_backdrops_below):
  /// the walk emits only the glass panels' backdrop layers, faded by the
  /// pass's accumulated opacity, and draws nothing else.
  pub backdrop_pass: Option<BackdropPass>,
}

/// The state of a backdrops-only pass (composite::emit_backdrops_below).
#[derive(Clone, Copy)]
pub struct BackdropPass {
  /// The product of the fading ancestors' opacities between the pass root
  /// and the walk's current node, applied to every panel emitted.
  pub alpha: f32,
  /// The fading node the pass was started for; its own backdrop is the
  /// regular composite path's to emit, so the pass skips it.
  pub root: u64,
}

impl<'a> BuildContext<'a> {
  pub fn new(platform: &'a PlatformContext, alloy: &'a crate::Context) -> Self {
    Self {
      platform,
      alloy,
      size: Size::default(),
      content: Rect::new(Point::zero(), Size::default()),
      cull: None,
      to_window: None,
      nodes_painted: 0,
      backdrop_regions: Vec::new(),
      boundaries_reused: 0,
      boundaries_recorded: 0,
      snapshots_reused: 0,
      snapshots_rerendered: 0,
      snapshots_rasterized: 0,
      backdrops_prepainted: 0,
      backdrop_pass: None,
    }
  }

  /// The per-walk counters an isolated descent (a capture, a reach under a
  /// cache) must not add to the frame's: taken before, put back after.
  pub(crate) fn walk_stats(&self) -> WalkStats {
    WalkStats {
      boundaries_reused: self.boundaries_reused,
      boundaries_recorded: self.boundaries_recorded,
      snapshots_reused: self.snapshots_reused,
      snapshots_rerendered: self.snapshots_rerendered,
      snapshots_rasterized: self.snapshots_rasterized,
      backdrops_prepainted: self.backdrops_prepainted,
    }
  }

  pub(crate) fn restore_walk_stats(&mut self, stats: WalkStats) {
    self.boundaries_reused = stats.boundaries_reused;
    self.boundaries_recorded = stats.boundaries_recorded;
    self.snapshots_reused = stats.snapshots_reused;
    self.snapshots_rerendered = stats.snapshots_rerendered;
    self.snapshots_rasterized = stats.snapshots_rasterized;
    self.backdrops_prepainted = stats.backdrops_prepainted;
  }
}

/// A snapshot of BuildContext's walk counters (see `walk_stats`).
#[derive(Clone, Copy)]
pub(crate) struct WalkStats {
  boundaries_reused: u32,
  boundaries_recorded: u32,
  snapshots_reused: u32,
  snapshots_rerendered: u32,
  snapshots_rasterized: u32,
  backdrops_prepainted: u32,
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

/// A replaced element's box from its intrinsic size and what layout already
/// knows - HTML's `<img>` rules, shared by the texture and the design-size view:
/// neither axis known -> the intrinsic size; one known -> the other follows
/// the intrinsic aspect ratio (the intrinsic extent when the ratio is
/// degenerate); both known -> both honored, the explicit override.
pub fn replaced_size(known: taffy::Size<Option<f32>>, intrinsic: Size) -> Size {
  let (iw, ih) = (intrinsic.width, intrinsic.height);
  match (known.width, known.height) {
    (Some(w), Some(h)) => Size::new(w, h),
    (Some(w), None) => Size::new(w, if iw > 0.0 { w * ih / iw } else { ih }),
    (None, Some(h)) => Size::new(if ih > 0.0 { h * iw / ih } else { iw }, h),
    (None, None) => Size::new(iw, ih),
  }
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
      ElementKind::Window(_) | ElementKind::View(_) | ElementKind::Span(_) => None,
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
      ElementKind::Window(_) | ElementKind::View(_) | ElementKind::Span(_) => None,
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
  /// `fallback`.
  pub fn local_bounds(&self, fallback: Size) -> Rect {
    match self {
      ElementKind::Rectangle(n) => n.local_bounds(fallback),
      ElementKind::Oval(n) => n.local_bounds(fallback),
      ElementKind::View(n) => n.local_bounds(fallback),
      ElementKind::Text(n) => n.local_bounds(fallback),
      ElementKind::Texture(n) => n.local_bounds(fallback),
      ElementKind::Line(n) => n.local_bounds(fallback),
      ElementKind::Path(n) => n.local_bounds(fallback),
      ElementKind::Window(_) | ElementKind::Span(_) => Rect::new(Point::zero(), fallback),
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
      ElementKind::Window(_) | ElementKind::View(_) | ElementKind::Span(_) => Size::zero(),
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
/// it; see composite::own_matrix and boundary::snapshot_node), but
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
  /// scroll are applied around it at composite time; see boundary::Hoist),
  /// but a Snapshot texture does not contain scrolled-out pixels and must
  /// re-rasterize.
  Scroll,
  /// Painted content changed; paint caches clear from the node up.
  Paint,
  /// Layout inputs changed; taffy caches and paint caches clear.
  Layout,
}

/// A painted frame's resolved screen damage, in window coordinates (logical
/// px): the union of every damaged node's old and new window extents,
/// produced by composite::paint_phase (okf/done/partial-repaint.md). `Full`
/// is the conservative fallback - resize, re-root, an unbounded or non-2D
/// extent in the union, or a damaged-node batch past the accumulation cap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameDamage {
  /// No painted pixel changed.
  None,
  /// Damage confined to this rect (already clamped to the window).
  Rect(Rect),
  Full,
}

impl FrameDamage {
  /// Damage area in logical px^2, with `window` as the full-frame answer.
  pub fn area(&self, window: Size) -> f32 {
    match self {
      FrameDamage::None => 0.0,
      FrameDamage::Rect(r) => r.size.width * r.size.height,
      FrameDamage::Full => window.width * window.height,
    }
  }
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
  // A snapshot boundary's retained rasterization vended as a registry
  // texture id (RenderTree::snapshot_texture). Allocated on first request,
  // stable for the element's lifetime; the paint walk re-publishes the
  // current backing under it after every rasterization.
  pub snapshot_texture_id: Cell<Option<u64>>,
  // The subtree's paint envelope (see cull.rs), cleared alongside paint_cache.
  pub envelope: cull::EnvelopeCache,
  // The subtree's window-space painted extent as of the last paint walk that
  // considered this node - the OLD half of a damaged node's old + new damage
  // union (okf/done/partial-repaint.md). Deliberately NOT cleared by
  // invalidate_paint: it must survive into the next frame's damage resolve,
  // and a stale value (a node inside a valid boundary cache) can only
  // over-damage, since whatever moved the subtree was damaged itself when it
  // moved. Empty until the walk first considers the node.
  pub last_extent: Cell<cull::Extent>,
  // Native transition declaration (see transitions.rs): which properties
  // animate on write, and how. None (the overwhelmingly common case) makes
  // every write snap, as ever.
  pub transitions: Option<Box<TransitionConfig>>,
  // The runtime state beside that declaration: shown, entered, exiting,
  // doomed, sliding (transitions.rs `Lifecycle`, each fact documented on
  // its field).
  pub lifecycle: Lifecycle,
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
      snapshot_texture_id: Cell::new(None),
      envelope: cull::EnvelopeCache::default(),
      last_extent: Cell::new(cull::Extent::Empty),
      transitions: None,
      lifecycle: Lifecycle::default(),
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
      snapshot_texture_id: Cell::new(None),
      envelope: cull::EnvelopeCache::default(),
      last_extent: Cell::new(cull::Extent::Empty),
      transitions: None,
      lifecycle: Lifecycle::default(),
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

  /// The box the node is painted at, in its parent's frame: its solved
  /// layout box, or the slide lane's while a layout slide runs
  /// (`Slide::at`). None for a detached node, which has no box of its own.
  /// Every consumer of a node's box - the paint walk, the envelope, hit
  /// testing, bounding boxes and so the tree dump - reads through here (or
  /// `placement`, `painted_size`, `content_box`, its projections), so they
  /// cannot disagree on where a sliding node is or how big. The solved box
  /// alone is `LayoutData::solved_box` (the offsetLeft-style `layout_box`
  /// query).
  pub fn painted_box(&self) -> Option<Rect> {
    let layout = self.layout.as_ref()?;
    Some(self.lifecycle.slide.and_then(|s| s.at).unwrap_or_else(|| layout.solved_box()))
  }

  /// Where the node is placed in its parent's frame (`painted_box`'s
  /// origin); zero for a detached node.
  pub fn placement(&self) -> Point {
    self.painted_box().map(|r| r.origin).unwrap_or_else(Point::zero)
  }

  /// The node's border box when laid out (`painted_box`'s size); None for a
  /// detached node.
  pub fn painted_size(&self) -> Option<Size> {
    self.painted_box().map(|r| r.size)
  }

  /// The element's frame: its border box when laid out, else the size it
  /// inherited (a detached node has no box of its own). The one spelling of
  /// the layout-size-else-inherited derivation every walk uses.
  pub fn frame_size(&self, inherited: Size) -> Size {
    self.painted_size().unwrap_or(inherited)
  }

  /// The painted border box inset by the layout's padding and border, origin
  /// included: the box the kind's own content sizes and places against
  /// (`LayoutData::content_box_of`). None for a detached node, whose content
  /// covers the frame it inherits.
  pub fn content_box(&self) -> Option<Rect> {
    let layout = self.layout.as_ref()?;
    Some(layout.content_box_of(self.painted_size().unwrap_or_else(|| layout.size())))
  }

  /// Whether this element references any texture-registry id: a texture
  /// element with a source, or a view whose shader samples extra texture
  /// inputs. Feeds the tree's referencer index (its membership predicate),
  /// which keeps `texture_content_changed` and the destroy sweep at
  /// O(referencers) instead of O(nodes).
  pub(crate) fn references_textures(&self) -> bool {
    match &self.kind {
      ElementKind::Texture(t) => t.texture_id.is_some(),
      ElementKind::View(v) => v.shader.as_ref().is_some_and(|s| !s.textures.is_empty()),
      _ => false,
    }
  }

  /// `display: none`: the subtree generates no box. Layout zeroes it (taffy's
  /// hidden pass) and the paint, hit and envelope walks never enter it, so a
  /// zero-frame text, an unbounded path or a detached child under it cannot
  /// draw or be hit. Detached elements have no style and are never hidden.
  pub fn is_hidden(&self) -> bool {
    self.layout.as_ref().is_some_and(|l| l.style.display == taffy::style::Display::None)
  }

  /// The style this element's kind starts layout with; a null layout-prop
  /// write resets the named field from here (each kind seeds its own
  /// defaults - a view's column direction, a texture's align-self). None for
  /// detached elements, which have no style.
  pub fn initial_style(&self) -> Option<Style> {
    if !self.has_layout() {
      return None;
    }
    Some(match &self.kind {
      ElementKind::Window(_) => Window::initial_style(),
      ElementKind::View(_) => View::initial_style(),
      ElementKind::Rectangle(_) => Rectangle::initial_style(),
      ElementKind::Oval(_) => Oval::initial_style(),
      ElementKind::Line(_) => Line::initial_style(),
      ElementKind::Path(_) => Path::initial_style(),
      ElementKind::Text(_) => Text::initial_style(),
      ElementKind::Texture(_) => Texture::initial_style(),
      ElementKind::Span(_) => return None,
    })
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
