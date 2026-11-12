//! Damage rectangles for flush-rendered draw targets, the pure half (D8 of
//! okf/design/draw-order-and-culling.md). A target's pending dirt is full or
//! a small set of pixel rectangles; a rectangle-dirty target renders only
//! inside its rectangles and keeps every other pixel. The rectangles come
//! from a draw list: the cull boxes of the records a write touched, before
//! and after, projected through the list's view into the target's pixels
//! (`ListDamage`).
//!
//! The pixel space is a target's image space: top-left origin, row 0 =
//! clip y -1 (the pixel contract of `@solidrt/core/gpu`). It is the space a
//! sub-target's `Region` rectangle uses and the GL viewport space of a
//! target, unflipped. A sub-target's rectangles are in its own space,
//! origin at its own top-left corner.
//!
//! No GL here: the raster thread carries out what these rectangles say
//! (raster/targets.rs, gl/target.rs).

use super::draw_list::CellIndex;
use super::order::{CellBox, OrderView, VIEW_W_FLOOR};

/// The most rectangles one owner's damage holds; past it they collapse to
/// their bounding rectangle. Each rectangle is one group of the owner's
/// pass and re-issues every draw of the owner, so the vertex work
/// multiplies by the count.
pub const MAX_DAMAGE_RECTS: usize = 4;

/// The share of an owner's area above which its damage renders in full: a
/// pass without a clear loads the whole surface on a tiled GPU, so a mostly
/// damaged owner gains nothing from the partial path.
pub const DAMAGE_FULL_RATIO: f64 = 0.5;

/// Pixels a projected box grows by on every side: the rasterization edge,
/// where a fragment whose center sits just outside the projected bounds is
/// still covered by an edge that rounds across them.
pub const DAMAGE_EDGE_PX: f64 = 1.0;

/// Pixels within which a projected edge counts as on a pixel boundary
/// before it rounds outward: an f32 view matrix lands an exact boundary a
/// few ulps off, which outward rounding alone would grow by a whole pixel.
/// Far below the `DAMAGE_EDGE_PX` margin, so it never loses a fragment.
pub const DAMAGE_ROUND_EPS_PX: f64 = 1e-3;

/// The most slots one refresh of a draw list derives its damage from;
/// past it the refresh is a bulk change and renders in full. It bounds the
/// per-frame box maths (two projections per slot) of a churn, where the
/// rectangles would collapse to one bounding rectangle anyway.
pub const MAX_DAMAGE_SLOTS: usize = 64;

/// A rectangle of a target's pixels, in its image space (see the module
/// doc). Empty when either side is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
  pub x: i32,
  pub y: i32,
  pub w: u32,
  pub h: u32,
}

impl PxRect {
  pub fn is_empty(&self) -> bool {
    self.w == 0 || self.h == 0
  }

  fn right(&self) -> i64 {
    self.x as i64 + self.w as i64
  }

  fn bottom(&self) -> i64 {
    self.y as i64 + self.h as i64
  }

  // The rectangle from its edges; empty where they cross.
  fn from_edges(x0: i64, y0: i64, x1: i64, y1: i64) -> PxRect {
    PxRect { x: x0 as i32, y: y0 as i32, w: (x1 - x0).max(0) as u32, h: (y1 - y0).max(0) as u32 }
  }

  /// The pixels it covers.
  pub fn area(&self) -> u64 {
    self.w as u64 * self.h as u64
  }

  /// Whether the two overlap or share an edge or a corner: the rectangles
  /// a set merges.
  pub fn meets(&self, other: &PxRect) -> bool {
    self.x as i64 <= other.right()
      && other.x as i64 <= self.right()
      && self.y as i64 <= other.bottom()
      && other.y as i64 <= self.bottom()
  }

  /// The bounding rectangle of both.
  pub fn union(&self, other: &PxRect) -> PxRect {
    PxRect::from_edges(
      (self.x as i64).min(other.x as i64),
      (self.y as i64).min(other.y as i64),
      self.right().max(other.right()),
      self.bottom().max(other.bottom()),
    )
  }

  /// The part both cover; None when they do not overlap.
  pub fn intersect(&self, other: &PxRect) -> Option<PxRect> {
    let cut = PxRect::from_edges(
      (self.x as i64).max(other.x as i64),
      (self.y as i64).max(other.y as i64),
      self.right().min(other.right()),
      self.bottom().min(other.bottom()),
    );
    (!cut.is_empty()).then_some(cut)
  }

  /// The part inside a `width` x `height` owner; None when nothing is.
  pub fn clipped(&self, width: u32, height: u32) -> Option<PxRect> {
    self.intersect(&PxRect { x: 0, y: 0, w: width, h: height })
  }

  /// The rectangle moved by (`dx`, `dy`).
  pub fn translated(&self, dx: i32, dy: i32) -> PxRect {
    PxRect { x: self.x + dx, y: self.y + dy, ..*self }
  }
}

/// One owner's damage: at most `MAX_DAMAGE_RECTS` rectangles, none empty,
/// none meeting another (a rectangle added merges with every one it
/// overlaps or touches). One owner renders one group per rectangle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DamageRects {
  rects: Vec<PxRect>,
}

impl DamageRects {
  pub fn new() -> DamageRects {
    DamageRects::default()
  }

  pub fn rects(&self) -> &[PxRect] {
    &self.rects
  }

  pub fn is_empty(&self) -> bool {
    self.rects.is_empty()
  }

  /// Add `rect`: an empty one is dropped, one that meets rectangles of the
  /// set merges with them into their bounding rectangle, and past
  /// `MAX_DAMAGE_RECTS` the whole set collapses to its bounding rectangle.
  pub fn add(&mut self, rect: PxRect) {
    if rect.is_empty() {
      return;
    }
    let mut rect = rect;
    while let Some(at) = self.rects.iter().position(|r| r.meets(&rect)) {
      rect = rect.union(&self.rects.swap_remove(at));
    }
    self.rects.push(rect);
    if self.rects.len() > MAX_DAMAGE_RECTS {
      let bound = self.rects.iter().skip(1).fold(self.rects[0], |b, r| b.union(r));
      self.rects = vec![bound];
    }
  }

  /// Add every rectangle of `rects`.
  pub fn extend(&mut self, rects: impl IntoIterator<Item = PxRect>) {
    for rect in rects {
      self.add(rect);
    }
  }

  /// Cut every rectangle to a `width` x `height` owner, dropping what falls
  /// outside it.
  pub fn clip(&mut self, width: u32, height: u32) {
    self.rects = self.rects.iter().filter_map(|r| r.clipped(width, height)).collect();
  }

  /// The pixels the rectangles cover (they never overlap).
  pub fn area(&self) -> u64 {
    self.rects.iter().map(PxRect::area).sum()
  }

  /// Whether the damage covers more than `DAMAGE_FULL_RATIO` of a `width` x
  /// `height` owner: such an owner renders in full.
  pub fn covers(&self, width: u32, height: u32) -> bool {
    self.area() as f64 > DAMAGE_FULL_RATIO * (width as f64 * height as f64)
  }

  pub fn into_vec(self) -> Vec<PxRect> {
    self.rects
  }
}

/// A target's dirt as a draw list's write declares it: full, or only the
/// pixels inside these rectangles (in the target's own image space).
#[derive(Clone, Debug, PartialEq)]
pub enum TargetDirt {
  Full,
  Rects(Vec<PxRect>),
}

/// What one cull box covers on a target (`project_box`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxDamage {
  /// Nothing on the target: an empty box, or one wholly outside.
  Outside,
  Rect(PxRect),
  /// No bound: a corner is not finite, or sits at or behind the eye. The
  /// caller renders in full.
  Unbounded,
}

/// The pixels of a `target`-sized target (width, height) that the records
/// of cull box `b` (bounds in the record frame) may draw under `view`: the
/// box's eight corners through the view's clip matrix, to normalized
/// device coordinates, to the target's pixels (x right, y down from clip y
/// -1), their bounding rectangle grown by the box's screen reach and by
/// `DAMAGE_EDGE_PX`, rounded outward (past `DAMAGE_ROUND_EPS_PX` of float
/// noise) and cut to the target. The reach is
/// in `viewport` pixels (the pixels the list measures it in), which differ
/// from the target's when the target renders at an oversample: it scales
/// per axis by target size over viewport size.
pub fn project_box(view: &OrderView, b: &CellBox, viewport: [f32; 2], target: [f32; 2]) -> BoxDamage {
  if b.is_empty() {
    return BoxDamage::Outside;
  }
  let m = view.clip.map(|v| v as f64);
  let (tw, th) = (target[0] as f64, target[1] as f64);
  let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
  for corner in 0..8 {
    let x = if corner & 1 == 0 { b.bounds[0] } else { b.bounds[3] } as f64;
    let y = if corner & 2 == 0 { b.bounds[1] } else { b.bounds[4] } as f64;
    let z = if corner & 4 == 0 { b.bounds[2] } else { b.bounds[5] } as f64;
    let row = |r: usize| m[r] * x + m[4 + r] * y + m[8 + r] * z + m[12 + r];
    let (cx, cy, cw) = (row(0), row(1), row(3));
    if !(cx.is_finite() && cy.is_finite() && cw.is_finite() && cw > VIEW_W_FLOOR as f64) {
      return BoxDamage::Unbounded;
    }
    let px = (cx / cw * 0.5 + 0.5) * tw;
    let py = (cy / cw * 0.5 + 0.5) * th;
    x0 = x0.min(px);
    y0 = y0.min(py);
    x1 = x1.max(px);
    y1 = y1.max(py);
  }
  let reach = b.px as f64;
  if !reach.is_finite() {
    return BoxDamage::Unbounded;
  }
  let per_axis = |t: f64, v: f32| if v > 0.0 { t / v as f64 } else { 1.0 };
  let gx = reach.max(0.0) * per_axis(tw, viewport[0]) + DAMAGE_EDGE_PX;
  let gy = reach.max(0.0) * per_axis(th, viewport[1]) + DAMAGE_EDGE_PX;
  let left = (x0 - gx + DAMAGE_ROUND_EPS_PX).floor().max(0.0);
  let top = (y0 - gy + DAMAGE_ROUND_EPS_PX).floor().max(0.0);
  let right = (x1 + gx - DAMAGE_ROUND_EPS_PX).ceil().min(tw.floor());
  let bottom = (y1 + gy - DAMAGE_ROUND_EPS_PX).ceil().min(th.floor());
  if !(right > left && bottom > top) {
    return BoxDamage::Outside;
  }
  BoxDamage::Rect(PxRect::from_edges(left as i64, top as i64, right as i64, bottom as i64))
}

/// One rebuild point of a draw list, as its target's dirt is derived from
/// it (`dirt`): the list's state before the point updates it, and the
/// slots its buffers changed since the last point.
///
/// The contract the projection rests on: `view` is the matrix the pass
/// renders the list's records with (the vertex stage's clip transform), to
/// within the `DAMAGE_EDGE_PX` edge. A rectangle derived through any other
/// matrix names pixels the records do not draw and misses the ones they
/// do. So a view feed that lags the rendered matrix must not drive a
/// culling list: the gated order feed (`view_feed_pass` in spatial/mod.rs)
/// is one, since it skips small turns and moves. Nothing combines them
/// today, and nothing enforces it.
pub struct ListDamage<'a> {
  /// The population's cells; None when the list does not cull (no boxes).
  pub index: Option<&'a CellIndex>,
  /// The view the list culls under; None until fed.
  pub view: Option<&'a OrderView>,
  /// The view, viewport, key or direction changed since the last build.
  pub stale: bool,
  /// The population of the last build; None before the first.
  pub built: Option<usize>,
  /// The population now.
  pub population: usize,
  /// The slots any of the list's buffers changed since the last point
  /// (ascending, each once).
  pub touched: &'a [u32],
  /// The cull boxes now, the whole column (`CELL_BOX_FLOATS` per slot).
  pub boxes: &'a [f32],
  /// The box a slot had at the last point, when the box column's mirror
  /// marked it since then (it was overwritten); None for a slot it did not
  /// mark, whose box now is its box at the point. Asked for the slots under
  /// consideration only, at most `MAX_DAMAGE_SLOTS` of them.
  pub old_box: &'a dyn Fn(u32) -> Option<CellBox>,
  /// The pixels the screen reach is measured in.
  pub viewport: [f32; 2],
  /// The target's size in pixels (a sub-target's own); None when unknown.
  pub target: Option<[f32; 2]>,
}

impl ListDamage<'_> {
  /// The target's dirt for this point: the rectangles of every slot to
  /// consider, its box at the last point and its box now, each grown by
  /// the cell index's margin as culling reads it (`CellIndex::read`) and
  /// projected (`project_box`). A slot past the last build's population
  /// had no box, one past the population now has none. A rebuild and a
  /// patch are alike: a record whose key changed draws under different
  /// neighbours only inside its own box, and a slot that joined or left
  /// the drawn set changes only its own box.
  ///
  /// Full when no rectangle can be trusted: the list does not cull, has
  /// no view, is stale or was never built, the target's size is unknown,
  /// more than `MAX_DAMAGE_SLOTS` slots are to consider (counted before
  /// anything is allocated), or a box projects without bound.
  pub fn dirt(&self) -> TargetDirt {
    let (Some(index), Some(view), false, Some(built), Some(target)) =
      (self.index, self.view, self.stale, self.built, self.target)
    else {
      return TargetDirt::Full;
    };
    // The slots to consider: the touched ones below the smaller
    // population, and every slot from there to the larger one (inside both
    // populations, or added or removed by a population change). `touched`
    // is ascending and unique, so the union is counted, not collected.
    let (low, reach) = (built.min(self.population), built.max(self.population));
    let below = self.touched.partition_point(|&s| (s as usize) < low);
    if below + (reach - low) > MAX_DAMAGE_SLOTS {
      return TargetDirt::Full;
    }
    let slots = self.touched[..below].iter().map(|&s| s as usize).chain(low..reach);
    let mut rects = DamageRects::new();
    for s in slots {
      let before = match (s < built, (self.old_box)(s as u32)) {
        (false, _) => CellBox::EMPTY,
        (true, Some(b)) => index.grow(b),
        (true, None) => index.read(self.boxes, s),
      };
      let after = if s < self.population { index.read(self.boxes, s) } else { CellBox::EMPTY };
      for b in [before, after] {
        match project_box(view, &b, self.viewport, target) {
          BoxDamage::Outside => {}
          BoxDamage::Rect(r) => rects.add(r),
          BoxDamage::Unbounded => return TargetDirt::Full,
        }
      }
    }
    TargetDirt::Rects(rects.into_vec())
  }
}
