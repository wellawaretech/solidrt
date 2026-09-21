// Paint viewport culling (okf/backlog/paint-viewport-culling.md): the paint
// walk carries a cull rect (what can still be seen, in the current local
// space) and skips a child subtree when its paint envelope cannot intersect
// it. Everything here is conservative by construction: an unknown maps to
// "cull nothing" or "unbounded", never to a wrong skip.
//
// Spaces. A node has three frames the walk passes through in record order
// (composite::record_node): the SLOT frame it is placed in (its parent's
// child frame, translated to the node's location; the node's own matrix has
// not been applied), its BOX frame (after the own matrix; the layout box, the
// overflow clip and the scroll offset live here), and its CHILD frame (after
// scroll and design-size fit; children are placed here). Envelopes are stated in
// the slot frame so a parent can test them directly; the cull rect is carried
// in whichever frame the walk is currently in.
use std::cell::Cell;

use crate::impellers::{Matrix, Point, Rect, Size};
use crate::rendertree::{
  BoundaryMode, Bounded, Element, ElementKind, PlatformContext, RenderTree, ShadowState, Vector,
};
use taffy::style::Overflow;

// Half-extent standing in for "no bound on this axis": large enough to cover
// any window, small enough to stay exact in f32 arithmetic.
const HALF_INF: f32 = 1.0e7;

// Antialiasing bleeds a fraction of a pixel past a shape's geometry; every
// own extent grows by this much.
const AA_OUTSET: f32 = 1.0;

/// Conservative painted extent of a subtree, in the node's slot frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extent {
  /// Nothing is painted (a bare container with no children).
  Empty,
  Bounded(Rect),
  /// The extent is not known: the subtree is never culled and neither is any
  /// ancestor up to the nearest clipping one.
  Unbounded,
}

/// Forward companion of the cull rect: the 2D transform from the frame the
/// paint walk is currently in to window space (logical px). `None` once a
/// non-2D matrix entered the chain - extents mapped through it become
/// Unbounded, so a 3D subtree degrades to full damage, never to a wrong
/// skip. Carried by BuildContext for the damage-extent cells
/// (okf/done/partial-repaint.md); unlike the cull rect it never suspends
/// inside boundary recordings, since a recording replays at the walk's
/// current window position.
pub type WindowMap = Option<euclid::default::Transform2D<f32>>;

/// `map` one record-order op deeper: the walk applied `m` (own matrix or
/// design-size fit) to the frame it is in.
pub(crate) fn map_through(map: &WindowMap, m: &Matrix) -> WindowMap {
  let cur = (*map)?;
  if !m.is_2d() {
    return None;
  }
  Some(m.to_2d().then(&cur))
}

/// `map` past a translation the walk recorded (a scroll's -offset, a
/// child's location).
pub(crate) fn map_translate(map: &WindowMap, v: Vector) -> WindowMap {
  map.map(|m| m.pre_translate(v))
}

impl Extent {
  pub(crate) fn union(self, other: Extent) -> Extent {
    match (self, other) {
      (Extent::Unbounded, _) | (_, Extent::Unbounded) => Extent::Unbounded,
      (Extent::Empty, e) | (e, Extent::Empty) => e,
      (Extent::Bounded(a), Extent::Bounded(b)) => Extent::Bounded(a.union(&b)),
    }
  }

  fn translate(self, by: Vector) -> Extent {
    match self {
      Extent::Bounded(r) => Extent::Bounded(r.translate(by)),
      e => e,
    }
  }

  // Maps a bounded extent forward through `m` (its outer bounding box);
  // anything but a 2D affine matrix makes the result unknown.
  pub(crate) fn transformed(self, m: &Matrix) -> Extent {
    match self {
      Extent::Bounded(r) => {
        if !m.is_2d() {
          return Extent::Unbounded;
        }
        Extent::Bounded(m.to_2d().outer_transformed_rect(&r))
      }
      e => e,
    }
  }

  /// The extent in window space: placed at its slot position `pos`, then
  /// through the walk's forward window map. An unknown map makes any
  /// painted extent Unbounded.
  pub(crate) fn to_window(self, pos: Point, map: &WindowMap) -> Extent {
    match (self.translate(pos.to_vector()), map) {
      (Extent::Empty, _) => Extent::Empty,
      (Extent::Bounded(r), Some(m)) => Extent::Bounded(m.outer_transformed_rect(&r)),
      (_, None) | (Extent::Unbounded, _) => Extent::Unbounded,
    }
  }

  /// True when something in the extent may lie inside `cull`.
  pub fn may_intersect(self, cull: &Rect) -> bool {
    match self {
      Extent::Empty => false,
      Extent::Unbounded => true,
      Extent::Bounded(r) => r.intersects(cull),
    }
  }
}

/// Per-element cache of the envelope, and beside it of `backdrop_below`,
/// which the same invalidation walks keep current. Interior-mutable because
/// painting traverses a shared tree; the envelope is keyed on the frame size
/// the node inherited, since a detached node's own extent resolves against
/// it.
#[derive(Default)]
pub struct EnvelopeCache {
  envelope: Cell<Option<(Size, Extent)>>,
  backdrop_below: Cell<Option<bool>>,
}

impl EnvelopeCache {
  pub fn clear(&self) {
    self.envelope.set(None);
    self.backdrop_below.set(None);
  }
}

/// Whether a glass panel (a view with a backdrop filter) paints somewhere
/// below `node_id`, the node itself excluded: the gate for
/// composite::emit_backdrops_below, so a fading subtree without glass costs
/// nothing extra. Hidden subtrees are not looked into, nor are backdrop
/// roots (a snapshot boundary, a filtered view): their panels never read the
/// window, so there is nothing to emit for them. Cached on the element and
/// cleared by every paint invalidation that reaches it - an insert, a
/// removal, a backdrop or filter write anywhere below all walk up through
/// invalidate_paint - so a change recomputes only the ancestor chain.
pub fn backdrop_below(scene: &RenderTree, node_id: u64) -> bool {
  let element = scene.node(node_id);
  if let Some(below) = element.envelope.backdrop_below.get() {
    return below;
  }
  let below = element.children.iter().any(|&child_id| {
    let child = scene.node(child_id);
    if child.is_hidden() {
      return false;
    }
    let panel = matches!(&child.kind, ElementKind::View(v) if v.active_backdrop_filter().is_some());
    panel || (!is_backdrop_root(child) && backdrop_below(scene, child_id))
  });
  element.envelope.backdrop_below.set(Some(below));
  below
}

/// A node whose subtree's backdrop panels read an offscreen of its own
/// rather than the window: a snapshot boundary (its raster) or a filtered
/// view (its effect layer). Its own backdrop is still live - emitted before
/// the raster or the layer - so a root is looked at, never into.
pub(crate) fn is_backdrop_root(element: &Element) -> bool {
  let filtered = matches!(&element.kind, ElementKind::View(v) if v.active_filter().is_some());
  filtered || matches!(element.repaint_boundary, BoundaryMode::Snapshot | BoundaryMode::SnapshotNoAa)
}

// The frame a node's detached children inherit: its own layout box (design
// size under a design size), else what it inherited itself. Mirrors the child
// walk in composite::record_node.
pub(crate) fn child_frame(element: &Element, inherited: Size) -> Size {
  let mut frame = element.frame_size(inherited);
  if let ElementKind::View(v) = &element.kind {
    if let Some(vb) = v.design_space() {
      frame = vb;
    }
  }
  frame
}

// What the node's own build() paints, in its box frame. Kinds default their
// geometry to the border box (`inherited` for a detached node), text to the
// content box - the same frames the paint walk hands BuildContext
// (okf/done/padding-box-divergence.md). A line's and a path's bounds already
// hold their stroke (the outset is geometry-dependent), so only AA is added.
// Text the extent cannot be read from is unbounded.
fn own_extent(element: &Element, platform: &PlatformContext, inherited: Size) -> Extent {
  let frame = element.frame_size(inherited);
  let content = element.content_box().unwrap_or(Rect::new(Point::zero(), frame));
  let inflate = |r: Rect, by: f32| Extent::Bounded(r.inflate(by, by));
  // A shape's shadow paints past its geometry: union the shadow's own
  // reach (offset + spread + blur falloff) into the extent.
  let with_shadow = |r: Rect, by: f32, shadow: &Option<ShadowState>| {
    let base = inflate(r, by);
    match shadow {
      Some(s) => base.union(inflate(s.extent_of(r), AA_OUTSET)),
      None => base,
    }
  };
  let mut extent = match &element.kind {
    ElementKind::Window(_) | ElementKind::Span(_) => Extent::Empty,
    // A backdrop panel paints the filtered backdrop across its box even
    // with no children - visible content of its own, so a detached one
    // must not resolve to an empty (cullable) extent.
    ElementKind::View(v) => match v.active_backdrop_filter() {
      Some(_) => Extent::Bounded(Rect::new(Point::zero(), frame)),
      None => Extent::Empty,
    },
    // A box kind's stroke paints inside its box (Rectangle::build), so the
    // box plus AA is the whole painted area at any stroke width.
    ElementKind::Rectangle(r) => with_shadow(r.local_bounds(frame), AA_OUTSET, &r.shadow),
    ElementKind::Oval(o) => with_shadow(o.local_bounds(frame), AA_OUTSET, &o.shadow),
    ElementKind::Texture(t) => inflate(t.local_bounds(frame), AA_OUTSET),
    ElementKind::Text(t) => match t.painted_extent(platform, content) {
      Some(r) => inflate(r, AA_OUTSET),
      None => Extent::Unbounded,
    },
    ElementKind::Line(l) => inflate(l.local_bounds(frame), AA_OUTSET),
    ElementKind::Path(p) => with_shadow(p.local_bounds(frame), AA_OUTSET, &p.shadow),
  };
  // A laid-out node's box is a harmless superset of what its own build draws
  // inside it, and it is what everything else (clip, hit) already means by
  // the node.
  if let Some(size) = element.painted_size() {
    extent = extent.union(Extent::Bounded(Rect::new(Point::zero(), size)));
  }
  extent
}

/// The subtree's paint envelope in the node's slot frame: its own painted
/// extent plus every child's envelope, cut to the box on each axis the node
/// clips, then through the node's own matrix. Cached on the element until
/// `RenderTree::invalidate_paint` reaches it (the same walk that drops
/// boundary recordings, so the two can never disagree about staleness).
pub fn envelope(scene: &RenderTree, node_id: u64, platform: &PlatformContext, inherited: Size) -> Extent {
  let element = scene.node(node_id);
  if let Some((size, extent)) = element.envelope.envelope.get() {
    if size == inherited {
      return extent;
    }
  }
  let extent = compute_envelope(scene, element, platform, inherited);
  element.envelope.envelope.set(Some((inherited, extent)));
  extent
}

fn compute_envelope(scene: &RenderTree, element: &Element, platform: &PlatformContext, inherited: Size) -> Extent {
  let box_size = element.painted_size();
  let (clip_x, clip_y) = element
    .layout
    .as_ref()
    .map(|l| (l.style.overflow.x != Overflow::Visible, l.style.overflow.y != Overflow::Visible))
    .unwrap_or((false, false));

  // Children, in the child frame, then back into the box frame (fit forward,
  // scroll back). A text's spans are drawn by the text; its atoms are laid
  // out and drawn like any child.
  let mut children = Extent::Empty;
  if !(clip_x && clip_y) {
    let text_atoms = matches!(&element.kind, ElementKind::Text(_));
    let frame = child_frame(element, inherited);
    for &child_id in &element.children {
      let child = scene.node(child_id);
      if child.is_hidden() {
        continue;
      }
      if text_atoms && !child.has_layout() {
        continue;
      }
      let pos = child.placement();
      children = children.union(envelope(scene, child_id, platform, frame).translate(pos.to_vector()));
      if children == Extent::Unbounded {
        break;
      }
    }
    if let ElementKind::View(v) = &element.kind {
      if let Some(fit) = v.fit_matrix(box_size.unwrap_or(inherited)) {
        children = children.transformed(&fit);
      }
      if let Some(s) = v.scroll {
        children = children.translate(-s);
      }
    }
  }

  let mut extent = own_extent(element, platform, inherited).union(children);

  // A clipped axis bounds the whole subtree to the box on that axis, whatever
  // the children claim.
  if let (Some(size), true) = (box_size, clip_x || clip_y) {
    let (x0, x1) = if clip_x { (0.0, size.width) } else { (-HALF_INF, HALF_INF) };
    let (y0, y1) = if clip_y { (0.0, size.height) } else { (-HALF_INF, HALF_INF) };
    let clip = Rect::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0));
    extent = match extent {
      Extent::Empty => Extent::Empty,
      Extent::Unbounded => Extent::Bounded(clip),
      Extent::Bounded(r) => r.intersection(&clip).map(Extent::Bounded).unwrap_or(Extent::Empty),
    };
  }

  // A view filter's blur paints past the subtree it filters; grow the
  // extent by its reach. After the clip cut (the blur samples the clipped
  // composite and softens outward from it), before the own matrix (the
  // filter is applied under the view's transform).
  if let ElementKind::View(v) = &element.kind {
    if let Some(f) = v.active_filter() {
      let reach = f.blur_outset();
      if reach > 0.0 {
        if let Extent::Bounded(r) = extent {
          extent = Extent::Bounded(r.inflate(reach, reach));
        }
      }
    }
  }

  // Into the slot frame through the node's own matrix (Views only).
  match &element.kind {
    ElementKind::View(v) => extent.transformed(&v.box_matrix(box_size.unwrap_or(inherited))),
    _ => extent,
  }
}

/// The cull rect one step further into the tree. `None` means nothing is
/// culled below. Each step mirrors one op of the record order.
pub trait CullRect {
  /// Into a child placed at `pos` (the walk's per-child translate).
  fn into_child(&self, pos: Point) -> Self;
  /// Through a matrix the walk applies (own matrix, design-size fit): the rect is
  /// mapped by the inverse; a non-invertible or non-2D matrix means unknown.
  fn through(&self, m: &Matrix) -> Self;
  /// Under an overflow clip on the given axes of a `size` box.
  fn clipped(&self, size: Size, clip_x: bool, clip_y: bool) -> Self;
  /// Past a scroll offset (children slide by -scroll, so the visible window
  /// moves by +scroll in their frame).
  fn scrolled(&self, scroll: Vector) -> Self;
}

impl CullRect for Option<Rect> {
  fn into_child(&self, pos: Point) -> Self {
    self.map(|r| r.translate(-pos.to_vector()))
  }

  fn through(&self, m: &Matrix) -> Self {
    let r = (*self)?;
    if !m.is_2d() {
      return None;
    }
    let inv = m.to_2d().inverse()?;
    Some(inv.outer_transformed_rect(&r))
  }

  fn clipped(&self, size: Size, clip_x: bool, clip_y: bool) -> Self {
    if !clip_x && !clip_y {
      return *self;
    }
    let (x0, x1) = if clip_x { (0.0, size.width) } else { (-HALF_INF, HALF_INF) };
    let (y0, y1) = if clip_y { (0.0, size.height) } else { (-HALF_INF, HALF_INF) };
    let clip = Rect::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0));
    // An empty intersection is a valid cull rect (nothing visible), not an
    // unknown one.
    Some(match self {
      Some(r) => r.intersection(&clip).unwrap_or(Rect::zero()),
      None => clip,
    })
  }

  fn scrolled(&self, scroll: Vector) -> Self {
    self.map(|r| r.translate(scroll))
  }
}
