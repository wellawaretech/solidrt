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
// scroll and viewBox fit; children are placed here). Envelopes are stated in
// the slot frame so a parent can test them directly; the cull rect is carried
// in whichever frame the walk is currently in.
use std::cell::Cell;

use crate::impellers::{Matrix, Point, Rect, Size};
use crate::rendertree::{Bounded, Element, ElementKind, PlatformContext, RenderTree, Vector};
use taffy::style::Overflow;

// Half-extent standing in for "no bound on this axis": large enough to cover
// any window, small enough to stay exact in f32 arithmetic.
const HALF_INF: f32 = 1.0e7;

// Antialiasing and hairline strokes bleed a fraction of a pixel past a shape's
// geometry; every own extent grows by this much.
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

impl Extent {
  fn union(self, other: Extent) -> Extent {
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
  fn transformed(self, m: &Matrix) -> Extent {
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

  /// True when something in the extent may lie inside `cull`.
  pub fn may_intersect(self, cull: &Rect) -> bool {
    match self {
      Extent::Empty => false,
      Extent::Unbounded => true,
      Extent::Bounded(r) => r.intersects(cull),
    }
  }
}

/// Per-element cache of the envelope. Interior-mutable because painting
/// traverses a shared tree; keyed on the frame size the node inherited, since a
/// detached node's own extent resolves against it.
#[derive(Default)]
pub struct EnvelopeCache(Cell<Option<(Size, Extent)>>);

impl EnvelopeCache {
  pub fn clear(&self) {
    self.0.set(None);
  }
}

// The frame a node's detached children inherit: its own layout box (design
// size under a viewBox), else what it inherited itself. Mirrors the child
// walk in composite::record_node.
pub(crate) fn child_frame(element: &Element, inherited: Size) -> Size {
  let mut frame = element.layout.as_ref().map(|l| l.size()).unwrap_or(inherited);
  if let ElementKind::View(v) = &element.kind {
    if let Some(vb) = v.view_box {
      frame = vb;
    }
  }
  frame
}

// The frame a node's own build() reads: the content box when laid out
// (padding subtracted, as the child walk sets ctx.size), else the inherited
// frame.
fn own_frame(element: &Element, inherited: Size) -> Size {
  match &element.layout {
    Some(l) => {
      let c = &l.computed;
      Size::new(c.size.width - c.padding.left - c.padding.right, c.size.height - c.padding.top - c.padding.bottom)
    }
    None => inherited,
  }
}

// What the node's own build() paints, in its box frame. Kinds that draw in
// their own coordinate space (Line, Path) or through an engine the extent
// cannot be read from are unbounded.
fn own_extent(element: &Element, platform: &PlatformContext, frame: Size) -> Extent {
  let inflate = |r: Rect, by: f32| Extent::Bounded(r.inflate(by, by));
  let mut extent = match &element.kind {
    ElementKind::Window(_) | ElementKind::View(_) | ElementKind::Span(_) => Extent::Empty,
    ElementKind::Rectangle(r) => inflate(r.local_bounds(frame), AA_OUTSET + r.paint.stroke_width),
    ElementKind::Oval(o) => inflate(o.local_bounds(frame), AA_OUTSET + o.paint.stroke_width),
    ElementKind::Texture(t) => inflate(t.local_bounds(frame), AA_OUTSET),
    ElementKind::Text(t) => match t.painted_extent(platform, frame) {
      Some(r) => inflate(r, AA_OUTSET),
      None => Extent::Unbounded,
    },
    ElementKind::Line(_) | ElementKind::Path(_) => Extent::Unbounded,
  };
  // A laid-out node's box is a harmless superset of what its own build draws
  // inside it, and it is what everything else (clip, hit) already means by
  // the node.
  if let Some(l) = &element.layout {
    extent = extent.union(Extent::Bounded(Rect::new(Point::zero(), l.size())));
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
  if let Some((size, extent)) = element.envelope.0.get() {
    if size == inherited {
      return extent;
    }
  }
  let extent = compute_envelope(scene, element, platform, inherited);
  element.envelope.0.set(Some((inherited, extent)));
  extent
}

fn compute_envelope(scene: &RenderTree, element: &Element, platform: &PlatformContext, inherited: Size) -> Extent {
  let box_size = element.layout.as_ref().map(|l| l.size());
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
    let text_atoms = match &element.kind {
      ElementKind::Text(t) => Some(!t.paragraph_engine),
      _ => None,
    };
    let frame = child_frame(element, inherited);
    for &child_id in &element.children {
      let child = scene.node(child_id);
      if let Some(atoms) = text_atoms {
        if !atoms || !child.has_layout() {
          continue;
        }
      }
      let pos = child.layout.as_ref().map(|l| l.location()).unwrap_or_default();
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

  let mut extent = own_extent(element, platform, own_frame(element, inherited)).union(children);

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
  /// Through a matrix the walk applies (own matrix, viewBox fit): the rect is
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
