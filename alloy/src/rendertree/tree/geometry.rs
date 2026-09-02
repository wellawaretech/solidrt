//! Node geometry queries: bounding boxes (window- and viewport-relative),
//! layout boxes, painted quads, and the transform walk they share - the
//! read-only geometry surface inspection and hit consumers ask of the tree.

use crate::impellers::Matrix;
use super::RenderTree;
use crate::rendertree::{ElementKind, Point, Rect, Size};

impl RenderTree {
  /// Bounding box of a node relative to its nearest positioning context: the
  /// closest ancestor whose JSX explicitly set `position="relative"`. Falls
  /// back to the window when there is none. This is the frame an absolutely
  /// positioned sibling overlay is drawn in, so coordinates from here can feed
  /// directly into such an overlay. Detached nodes report the box inherited from
  /// their nearest laid-out ancestor. Returns None before the first layout.
  pub fn bounding_box(&self, id: u64) -> Option<Rect> {
    self.compute_bounding_box(id, true)
  }

  /// Bounding box of a node relative to the window root (CSS getBoundingClientRect
  /// semantics), for callers that want absolute coordinates (e.g. snapshot).
  pub fn bounding_box_viewport(&self, id: u64) -> Option<Rect> {
    self.compute_bounding_box(id, false)
  }

  /// The node's laid-out box in its parent's frame: layout output before any
  /// transform composes (DOM offsetLeft/offsetTop/offsetWidth/offsetHeight
  /// semantics). The untransformed companion to bounding_box: pointer events
  /// report local coordinates in this box's units, whatever designSize fits
  /// or transforms sit on the ancestor chain. None for detached nodes and
  /// before the first layout.
  pub fn layout_box(&self, id: u64) -> Option<Rect> {
    let node = self.try_node(id)?;
    let layout = node.layout.as_ref()?;
    if layout.cache.is_empty() {
      return None;
    }
    Some(Rect::new(layout.location(), layout.size()))
  }

  /// Computed lazily: walks from the node upward each call, so nothing is cached
  /// and only queried nodes cost anything. Call after layout_phase (e.g. from
  /// the postLayout hook) for current-frame values. When `stop_at_context` is
  /// set the ascent stops at (and does not fold in) the first positioning
  /// context ancestor, yielding coordinates in that ancestor's frame; otherwise
  /// it continues to the root.
  ///
  /// The four corners of the node's box are carried up through every ancestor:
  /// layout position, scroll (inverse), and the full View paint matrix
  /// (translate, rotate, scale, 3D) all compose, the forward companion of the
  /// hit-test descent. The result is the axis-aligned bounds of the transformed
  /// quad (CSS getBoundingClientRect semantics). Corners transform on the
  /// z = 0 plane with the homogeneous divide, the same approximation hit
  /// testing uses under perspective. Views without matrix props keep the cheap
  /// translation-only path.
  fn compute_bounding_box(&self, id: u64, stop_at_context: bool) -> Option<Rect> {
    // The axis-aligned bounds of the transformed quad.
    self.compute_corners(id, stop_at_context).map(Rect::from_points)
  }

  /// The four corners of the node's painted box in window coordinates, after
  /// every transform on the ancestor chain - the quad `bounding_box_viewport`
  /// collapses to an AABB. Corner order follows `rect_corners` (top-left,
  /// top-right, bottom-right, bottom-left, pre-transform). Under a rotation
  /// or 3D transform the AABB alone says a transform happened but not where
  /// the edges landed; the quad is the readable form.
  pub fn painted_quad(&self, id: u64) -> Option<[Point; 4]> {
    self.compute_corners(id, false)
  }

  fn compute_corners(&self, id: u64, stop_at_context: bool) -> Option<[Point; 4]> {
    let node = self.try_node(id)?;
    let fallback = self.content_fallback(id)?;
    let local = match &node.kind {
      // A detached text has no box of its own: its bounds are the laid-out
      // paragraph. A laid-out text keeps the box answer (its element box).
      ElementKind::Text(t) if node.layout.is_none() => t.detached_bounds(fallback),
      _ => node.kind.local_bounds(fallback),
    };

    // A View's own paint matrix already contains its translate (which is what
    // local_bounds reports as its origin), so the matrix path starts from the
    // plain layout box and applies the matrix instead; every other case keeps
    // the kind's local offset. The view's OWN box transforms by the user chain
    // only (box_matrix): a design-size fit maps children into the box, it never
    // moves the box itself.
    let mut corners = match &node.kind {
      ElementKind::View(v) if v.needs_matrix() => {
        let m = v.box_matrix(local.size);
        rect_corners(&Rect::new(Point::zero(), local.size)).map(|p| transform_point(&m, p))
      }
      _ => rect_corners(&local),
    };

    // Detached nodes have no layout placement; they inherit position from the
    // ancestor walk below.
    if let Some(layout) = node.layout.as_ref() {
      let loc = layout.location().to_vector();
      for p in corners.iter_mut() {
        *p += loc;
      }
    }

    // Ascend. Per ancestor, in application order: remove any scroll it applies
    // to its children, apply its paint matrix (or just its translate when no
    // matrix props are set), then add its layout position to enter the next
    // frame up. For the container-relative box, stop before folding in the
    // first positioning context: the result is then expressed in that
    // ancestor's frame. Absolute ancestors are deliberately transparent here -
    // their offset is still accumulated, they just never act as the stop.
    let mut cur_id = id;
    loop {
      let Some(parent_id) = self.try_node(cur_id).and_then(|n| n.parent) else {
        break;
      };
      let Some(parent) = self.try_node(parent_id) else {
        break;
      };
      if stop_at_context && parent.layout.as_ref().is_some_and(|l| l.positioning_context) {
        break;
      }
      if let ElementKind::View(v) = &parent.kind {
        if v.scroll.is_some() {
          let size =
            parent.layout.as_ref().map(|l| l.size()).or_else(|| self.content_fallback(parent_id)).unwrap_or_default();
          // Scroll means box pixels; these corners are in the parent's child
          // frame (design space under a design-size fit), so the offset divides
          // by the fit scale, matching the hit descent and the paint order.
          let s = v.content_scroll(size);
          for p in corners.iter_mut() {
            *p -= s;
          }
        }
        if v.needs_matrix() {
          let size =
            parent.layout.as_ref().map(|l| l.size()).or_else(|| self.content_fallback(parent_id)).unwrap_or_default();
          let m = v.paint_matrix(size);
          for p in corners.iter_mut() {
            *p = transform_point(&m, *p);
          }
        } else if let Some(t) = v.translate {
          for p in corners.iter_mut() {
            *p += t;
          }
        }
      }
      if let Some(parent_layout) = parent.layout.as_ref() {
        let loc = parent_layout.location().to_vector();
        for p in corners.iter_mut() {
          *p += loc;
        }
      }
      cur_id = parent_id;
    }

    Some(corners)
  }

  /// Fallback size for shapes without explicit w/h: the nearest laid-out node's
  /// box (self, or the ancestor a detached subtree hangs from). None before the
  /// first layout has populated the cache.
  fn content_fallback(&self, id: u64) -> Option<Size> {
    let mut cur = id;
    loop {
      let node = self.try_node(cur)?;
      // A design-size ANCESTOR redefines the space its children draw in: the box
      // they inherit is the design size, which the fit matrix maps onto the
      // layout box during the ancestor walk. The node's own design_size does not
      // apply to itself (its own box is its layout box).
      if cur != id {
        if let ElementKind::View(v) = &node.kind {
          if let Some(vb) = v.design_space() {
            return Some(vb);
          }
        }
      }
      if let Some(layout) = node.layout.as_ref() {
        if layout.cache.is_empty() {
          return None;
        }
        return Some(layout.size());
      }
      cur = node.parent?;
    }
  }

}

// The four corners of a rectangle, clockwise from top-left.
fn rect_corners(r: &Rect) -> [Point; 4] {
  let (min, max) = (r.origin, r.origin + r.size);
  [min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)]
}

// Forward companion of View::transform_to_local: applies a paint matrix to a
// point on the z = 0 plane with the homogeneous divide. A degenerate w (only
// reachable under perspective; euclid returns None for w <= 0) leaves the
// point untransformed rather than poisoning the box with infinities.
fn transform_point(m: &Matrix, p: Point) -> Point {
  m.transform_point2d(p).unwrap_or(p)
}
