pub(crate) mod boundary;
pub mod composite;
pub mod counters;
pub mod cull;
pub(crate) mod damage;
pub mod frame;
pub(crate) mod grid;
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
  ShadowState, Shown, Texture, TextureFit, View, Window, DEFAULT_STROKE_WIDTH,
};
pub use layout::{base_style, LayoutContext, LayoutData};
pub use platform::{FontPayload, PlatformContext};
pub use router::{InputEvent, PointerKey, PointerRouter, RoutedKind, RoutedPointer};
pub use text::{
  FontStyle, FontWeight, OverflowWrap, RunOverrides, RunStyle, Span, Text, TextAlignment, TextAnchor, TextOverflow,
  TextRun, ATOM_CHAR,
};
pub use transitions::{
  AnimKind, AnimProp, AnimValue, Curve, Endpoint, Lifecycle, Slide, TransitionConfig, TransitionEntry, TransitionSpec,
};
pub use tree::{Match, NodeMatch, NodeQuery, NodeSnapshot, RenderTree, SlideRemaining};

use crate::renderer::DisplayListBuilder;
use ore::cursor::Cursor;
use std::cell::{Cell, RefCell};
use taffy::{AvailableSpace, Position, Style};

// The rendertree's geometry vocabulary is the renderer's (euclid), so the
// types unify with every draw call. taffy geometry appears only as layout
// input (MeasureContext) and is always written taffy-qualified.
pub use crate::renderer::{Point, Rect, Size, Vector};

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
  /// The Recording boundary whose recording the walk is recording now, the
  /// nearest one: its id and its frame's map to the window
  /// (cull::RecordingFrame). A texture's build states its draw relative to
  /// that frame (kinds::Shown). Set by the boundary's record walk and put
  /// back after it, like `to_window`; None outside any.
  pub recording: Option<(u64, cull::WindowMap)>,
  /// The transform from the walk's current frame to the device pixels of
  /// the target being rasterized, for the paint-time snap of axis-aligned
  /// boxes (see grid.rs): the window's through the display scale, a
  /// snapshot's or a capture's texture inside its raster. None past a
  /// non-2D matrix.
  pub grid: grid::GridMap,
  /// Whether the walk is under a `pixelSnap={false}` node
  /// (`Element::pixel_snap_off`): the one source of the grid's `unrounded`
  /// flag that survives a raster root, which builds its grid from this bit
  /// where a running track's flag clears.
  pub pixel_snap_off: bool,
  /// Set by a raster root (a snapshot, a capture) for the node it records,
  /// which takes it: that node's content sits at the texture's origin by
  /// construction, so it takes no box offset (composite::record_node).
  pub raster_root: bool,
  /// The text atlas frame this walk belongs to: what a text or a recording
  /// stamps its scale observation with (text::build_layer).
  pub frame: u64,
  /// An enclosing recording re-recording at rest told this walk the
  /// composite scale is at rest: every text below rasters at it
  /// (text::raster_scale), whatever its own observation says.
  pub scale_at_rest: bool,
  /// Content this walk drew so far that is made for one scale: text layers,
  /// and the stroke of every visibly painted rect that takes its geometry
  /// from the frame, whether the device-pixel floor changed its width or
  /// not (okf/done/border-width-device-pixels.md). A recording reads the delta
  /// across its record walk to know whether it must be made again when its
  /// scale changes. Only the recording reads it: the paint counters' text
  /// layers (counters.rs) are their own.
  pub scale_bound: u32,
  /// Set while the walk records a snapshot boundary's raster
  /// (boundary::record_raster): the snapshot owns the look at rest of what
  /// its texture holds, so a text or a recording inside notes no scale wait
  /// and asks for no frame of its own (text::build_layer, the Recording arm
  /// of composite::build_recursive). A capture's walk is not one.
  pub in_snapshot_raster: bool,
  /// Content made mid-motion that must be made again when the scale rests,
  /// drawn or kept by the walk inside a snapshot raster so far: an unhinted
  /// text layer on a display that hints, a recording whose own look waits.
  /// A snapshot or a recording holding such content carries its own into
  /// an enclosing one's count when it is reused. A snapshot or a recording
  /// reads the delta across its record walk to know it must be made again,
  /// for rest, once it is at rest.
  pub rest_pending: u32,
  /// Set by the walk for the node whose kind it is about to build: whether
  /// a running track moves that node's stroke width
  /// (RenderTree::stroke_width_moving). A kind's build sees only this
  /// context, so the walk hands the answer over here, beside `size`.
  pub stroke_width_moving: bool,
  /// Where the composite scale of this frame is headed: the product of the
  /// running scale transitions' ends over their current values along the
  /// chain (RenderTree::scale_transition_factor), 1 with none. A text or a
  /// snapshot below rasters for the larger of now and then
  /// (text::raster_density).
  pub scale_target: f32,
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
  /// Snapshot boundaries whose raster for density the raster budget
  /// deferred this frame, composited stretched (among the reused).
  pub snapshots_deferred: u32,
  /// Glass panels whose backdrop layer was emitted ahead of a fading
  /// ancestor's opacity group this frame (composite::emit_backdrops_below).
  pub backdrops_prepainted: u32,
  /// Set while a backdrops-only pass runs (composite::emit_backdrops_below):
  /// the walk emits only the glass panels' backdrop layers, faded by the
  /// pass's accumulated opacity, and draws nothing else.
  pub backdrop_pass: Option<BackdropPass>,
  /// The walk's snapshot raster budget (boundary::RasterBudget): the grants
  /// made ahead of it, the time its snapshot rasters took so far. Not a
  /// walk counter: an isolated descent's rasters take time too.
  pub(crate) raster_budget: boundary::RasterBudget,
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
      recording: None,
      grid: grid::GridMap::default(),
      pixel_snap_off: false,
      raster_root: false,
      frame: platform.text_atlas().frame(),
      scale_at_rest: false,
      scale_bound: 0,
      in_snapshot_raster: false,
      rest_pending: 0,
      stroke_width_moving: false,
      scale_target: 1.0,
      nodes_painted: 0,
      backdrop_regions: Vec::new(),
      boundaries_reused: 0,
      boundaries_recorded: 0,
      snapshots_reused: 0,
      snapshots_rerendered: 0,
      snapshots_rasterized: 0,
      snapshots_deferred: 0,
      backdrops_prepainted: 0,
      backdrop_pass: None,
      raster_budget: boundary::RasterBudget::for_walk(platform.snapshot_raster_budget(), Vec::new()),
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
      snapshots_deferred: self.snapshots_deferred,
      backdrops_prepainted: self.backdrops_prepainted,
    }
  }

  pub(crate) fn restore_walk_stats(&mut self, stats: WalkStats) {
    self.boundaries_reused = stats.boundaries_reused;
    self.boundaries_recorded = stats.boundaries_recorded;
    self.snapshots_reused = stats.snapshots_reused;
    self.snapshots_rerendered = stats.snapshots_rerendered;
    self.snapshots_rasterized = stats.snapshots_rasterized;
    self.snapshots_deferred = stats.snapshots_deferred;
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
  snapshots_deferred: u32,
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
/// too, at the cost of GPU memory and resolution-dependence). The mode says
/// what is kept; how a snapshot rasterizes is the element's own choice
/// (`Element::antialias`).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundaryMode {
  #[default]
  None,
  Recording,
  Snapshot,
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
  // How a snapshot boundary rasterizes: multisampled (the default), or
  // single-sample with `antialias={false}` (no multisampled scratch, no
  // resolve pass, but vector content - svg paths, rotated edges - comes out
  // hard-edged; the app author opts in per boundary). Read only by the
  // snapshot arm of the paint walk, inert on any other mode. Written through
  // `set_antialias`, which owns the damage.
  pub antialias: bool,
  // `freeze`: on a snapshot boundary (`frozen`), the snapshot its first
  // snapshot paint after the write takes is held while the subtree changes
  // (SnapshotCache::held): a paint invalidation walk that reaches the node
  // marks it stale and stops (RenderTree::invalidate_paint), and the paint
  // composites the held texture at any scale or box
  // (boundary::snapshot_node). Inert on any other mode. Written through
  // `set_freeze`, which owns the damage.
  pub freeze: bool,
  // A `freeze={true}` write the paint walk has not looked at yet: off a
  // snapshot boundary it warns once per write (composite::build_recursive),
  // as a shader declared without a snapshot does.
  freeze_dirty: Cell<bool>,
  // Whether the snapshot arm drew the node inline at its last paint, with no
  // texture (a box with no texels, a failed raster, nothing to record): its
  // content is then in the frame's list or an enclosing recording, not in a
  // picture of its own, so it holds nothing and is not `frozen` for the
  // invalidation walks. Set and cleared by the snapshot arm
  // (boundary::snapshot_node); false for a node it never painted.
  pub(crate) snapshot_inline: Cell<bool>,
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
  // As a Recording boundary: the grid scale inside its own matrix as the
  // composites saw it (text::ScaleSeen), so a scale that held over the
  // rest looks is at rest (composite.rs, text::raster_scale).
  pub scale_seen: Cell<Option<text::ScaleSeen>>,
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
  // A stable name the app gives the node, for whoever has to find it again
  // without anything visible to find it by (a test, a dev tool): reported
  // in snapshots and matched by `RenderTree::find`. Never affects layout,
  // paint or hit testing.
  pub label: Option<String>,
  // `pixelSnap={false}`: nothing in this node's subtree is rounded to the
  // device grid, in motion or at rest (grid.rs). A one-way off, inherited
  // down the paint walk; a write is Paint damage, and the boundary caches
  // below notice the change themselves (boundary.rs).
  pub pixel_snap_off: bool,
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
      antialias: true,
      freeze: false,
      freeze_dirty: Cell::new(false),
      snapshot_inline: Cell::new(false),
      float: None,
      clear: None,
      paint_cache: RefCell::new(None),
      scale_seen: Cell::new(None),
      snapshot_texture_id: Cell::new(None),
      envelope: cull::EnvelopeCache::default(),
      last_extent: Cell::new(cull::Extent::Empty),
      transitions: None,
      lifecycle: Lifecycle::default(),
      label: None,
      pixel_snap_off: false,
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
      antialias: true,
      freeze: false,
      freeze_dirty: Cell::new(false),
      snapshot_inline: Cell::new(false),
      float: None,
      clear: None,
      paint_cache: RefCell::new(None),
      scale_seen: Cell::new(None),
      snapshot_texture_id: Cell::new(None),
      envelope: cull::EnvelopeCache::default(),
      last_extent: Cell::new(cull::Extent::Empty),
      transitions: None,
      lifecycle: Lifecycle::default(),
      label: None,
      pixel_snap_off: false,
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

  /// CSS `contain: size` on or off (see LayoutData::contain_size): the node
  /// sizes as if it had no content. Layout elements only; a detached node has
  /// no box to contain.
  pub fn set_contain_size(&mut self, contain: bool) {
    let layout = self.layout.as_mut().expect("contain requires a layout element");
    layout.contain_size = contain;
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

  /// Sets the cursor shown while this element is on the hovered path (see
  /// HitConfig::cursor); `None` clears it. Interaction metadata only: never
  /// affects layout, paint, or hit testing.
  pub fn set_cursor(&mut self, cursor: Option<Cursor>) {
    self.interaction.get_or_insert_with(HitConfig::default).cursor = cursor;
  }

  /// Write what the node retains as a repaint boundary (`repaint_boundary`),
  /// returning the damage. The mode in force is no change: Damage::None.
  /// Another mode is Damage::Compose: the ancestors hold the node's old
  /// composited result and repaint, and the node's own snapshot survives,
  /// so a boundary switched off and back with nothing changed inside reuses
  /// its texture, and a change inside while off (which marks it stale)
  /// re-renders once at the switch back. Leaving Recording drops the
  /// recording (it costs nothing to make again) and a write to Recording
  /// drops a snapshot (the Recording arm would replace it), so the one new
  /// state is a node that is off and holds a snapshot
  /// (`holds_off_snapshot`), which the tree releases after an age
  /// (RenderTree::release_off_snapshots). A switch to a snapshot marks a
  /// declared boundary shader dirty: a params write or an input change
  /// while it was not a snapshot boundary reached no pass, so the pass
  /// re-runs once over the kept source. A kept snapshot is no longer held
  /// (`freeze`): a switch back to a snapshot with `freeze` set takes its
  /// picture at its next snapshot paint, as a freeze write does.
  pub fn set_repaint_boundary(&mut self, mode: BoundaryMode) -> Damage {
    let was = self.repaint_boundary;
    if was == mode {
      return Damage::None;
    }
    self.repaint_boundary = mode;
    if was == BoundaryMode::Recording || mode == BoundaryMode::Recording {
      self.paint_cache.get_mut().take();
    }
    if let Some(PaintCache::Snapshot(snap)) = self.paint_cache.get_mut() {
      snap.held = false;
    }
    if mode == BoundaryMode::Snapshot {
      if let ElementKind::View(v) = &self.kind {
        if v.shader.is_some() {
          v.mark_shader_dirty();
        }
      }
    }
    Damage::Compose
  }

  /// Write how a snapshot boundary rasterizes (`antialias`), returning the
  /// damage. The value in force is no change: Damage::None. Another value
  /// keeps the storage and changes the raster, so it is Damage::Paint where
  /// a snapshot is or may be shown from it: on a snapshot boundary, and on a
  /// node that is off and holds a snapshot for the switch back
  /// (`holds_off_snapshot`). The walk of that damage marks the snapshot
  /// stale, and its next snapshot paint re-renders it at the new setting,
  /// in place at the same box and density. The storage does not depend on
  /// the setting, so the snapshot's key does not hold it. Anywhere else the
  /// field is inert, and the write repaints nothing: Damage::None.
  pub fn set_antialias(&mut self, antialias: bool) -> Damage {
    if self.antialias == antialias {
      return Damage::None;
    }
    self.antialias = antialias;
    if self.repaint_boundary == BoundaryMode::Snapshot || self.holds_off_snapshot() {
      Damage::Paint
    } else {
      Damage::None
    }
  }

  /// Write whether a snapshot boundary holds its picture (`freeze`),
  /// returning the damage. The value in force is no change: Damage::None.
  /// Any other write drops the held bit, so a freeze takes its picture at
  /// the node's next snapshot paint, and an unfreeze lets the snapshot rules
  /// decide again.
  ///
  /// A freeze is Damage::Compose: the node is painted in the frame of the
  /// write, which takes the picture of the state at the end of that batch,
  /// whatever the order of its writes (a write inside before or after the
  /// freeze only marks the snapshot stale). An unfreeze is Damage::Paint
  /// where the node holds a stale snapshot (something inside changed while
  /// it was held): the repaint takes the frozen quad's extent and the live
  /// subtree's, and only this subtree is rasterized again. Otherwise it is
  /// Damage::Compose: the walk then applies the snapshot rules, which reuse
  /// the texture where its key matches and re-raster it as they say where
  /// the box or the density moved during the freeze.
  ///
  /// Off a snapshot boundary the field is inert (`frozen`), but a changing
  /// write still reports Damage::Compose, so the node is painted and a
  /// `freeze={true}` there warns once, at paint.
  pub fn set_freeze(&mut self, freeze: bool) -> Damage {
    if self.freeze == freeze {
      return Damage::None;
    }
    self.freeze = freeze;
    self.freeze_dirty.set(freeze);
    let stale = match self.paint_cache.get_mut() {
      Some(PaintCache::Snapshot(snap)) => {
        snap.held = false;
        !snap.valid
      }
      _ => false,
    };
    if !freeze && stale {
      Damage::Paint
    } else {
      Damage::Compose
    }
  }

  /// Whether the node is a snapshot boundary with `freeze` set: where a
  /// paint invalidation walk stops (RenderTree::invalidate_paint) and the
  /// paint holds its picture. A `freeze` on a node that is off or a
  /// Recording boundary stops nothing and holds nothing. Nor does one whose
  /// last snapshot paint drew it inline (`snapshot_inline`): what it shows
  /// lives where a write inside must reach. A node never painted (culled,
  /// hidden, under a held ancestor) shows nothing of it, and stops them.
  pub fn frozen(&self) -> bool {
    self.freeze && self.repaint_boundary == BoundaryMode::Snapshot && !self.snapshot_inline.get()
  }

  /// Whether the node is frozen (`frozen`) and holds a stale snapshot:
  /// something inside changed since its picture was taken, and the unfreeze
  /// repaints it (`set_freeze`). What the tree read shows as the stale mark.
  pub fn frozen_stale(&self) -> bool {
    self.frozen() && matches!(&*self.paint_cache.borrow(), Some(PaintCache::Snapshot(snap)) if !snap.valid)
  }

  /// Write whether the rounding to the device grid is off for the node and
  /// its subtree (`pixel_snap_off`, `pixelSnap={false}`), returning the
  /// damage. The value in force is no change: Damage::None. Otherwise
  /// Damage::Paint brings the walk to the node, and the boundary caches
  /// below notice the other rounding themselves (boundary.rs). On a frozen
  /// node (`frozen`) the setting also decides how its quad rounds in its
  /// parent's slot: its snapshot is marked stale (the held raster keeps the
  /// rounding it was made under) and the write is Damage::Compose, whose
  /// walk passes it from the parent.
  pub fn set_pixel_snap_off(&mut self, off: bool) -> Damage {
    if self.pixel_snap_off == off {
      return Damage::None;
    }
    self.pixel_snap_off = off;
    if self.frozen() {
      self.mark_snapshot_stale();
      Damage::Compose
    } else {
      Damage::Paint
    }
  }

  /// Mark the node's snapshot stale, keeping its storage: its pixels no
  /// longer show what the subtree paints. Nothing for a node holding none.
  pub(crate) fn mark_snapshot_stale(&self) {
    if let Some(PaintCache::Snapshot(snap)) = &mut *self.paint_cache.borrow_mut() {
      snap.valid = false;
    }
  }

  /// Take the pending `freeze={true}` write flag; the paint walk consumes it
  /// (composite::build_recursive), warning where the node is no snapshot
  /// boundary.
  pub(crate) fn take_freeze_dirty(&self) -> bool {
    self.freeze_dirty.replace(false)
  }

  /// Whether the node is no repaint boundary and holds a snapshot: a
  /// snapshot boundary switched off keeps its storage for the switch back
  /// (`set_repaint_boundary`), valid or stale, until the release age.
  pub(crate) fn holds_off_snapshot(&self) -> bool {
    self.repaint_boundary == BoundaryMode::None && matches!(&*self.paint_cache.borrow(), Some(PaintCache::Snapshot(_)))
  }

  pub fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    self.kind.build(ctx, builder);
  }
}
