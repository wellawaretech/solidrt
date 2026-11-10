// The device pixel grid (okf/done/pixel-snapped-paint-boxes.md, revised by
// okf/done/pixel-snap-at-draw.md): layout places nodes at fractional
// positions (a 16.98 px row puts its rows at 45.71, 62.69, 79.67), and a
// box painted there is resampled a fraction off on every edge - a 1 px
// border reads as two grey rows, a divider blurs, a snapshot's texture
// carries the blur to all of its content. The paint walk snaps instead, at
// paint time only, as browsers do: every axis-aligned box lands with its
// device origin on a whole pixel and its far edges on whole pixels too, so
// neighbours still share an edge without a seam, and content moves by
// under a pixel. Layout and hit testing never see the snapped boxes.
//
// A box is rounded once, where it is drawn: the walk's frames stay
// fractional, and only a boundary rounds a position ahead of the draw
// (its recording or its texture is fixed relative to its origin). What
// moves a group - a scroll offset, a translate, a slide - rounds the
// movement (`snap_move`), never the position it lands on, so the children
// keep their fractions and their one rounding stays stable.
//
// Rounding is off (`unrounded`) for a subtree a running transition moves,
// which is resampled every frame anyway and would wobble, and for a subtree
// under `pixelSnap={false}`: `shift`, `snap_box` and `snap_move` then leave
// their input as it is.
//
// The grid is the device pixels of the target being rasterized: the window
// (through the display scale) or, inside a snapshot raster or a capture,
// that texture. The GridMap is the transform from the walk's current frame
// to those pixels, carried by BuildContext beside the window map
// (cull::WindowMap), advanced through the same ops, and reset at every
// raster root. The window map stays the damage side's: it keeps describing
// the window even inside a raster, which the grid must not.
//
// Nothing snaps under a rotation or a 3d transform: there is no grid to
// land on, and the map (None past a 3d matrix, rotated past a 2d rotation)
// says so. A scale is fine: the grid is finer or coarser in frame units,
// but it is still a grid.
use crate::renderer::{Matrix, Point, Size};
use crate::rendertree::Vector;

/// How far a rounded edge can sit from the fractional frame it was rounded
/// in, in device pixels: half a pixel, the most a rounding moves. What the
/// cull rect, stated in the fractional frame, grows by so it still holds
/// everything a snapped clip shows.
pub(crate) const SNAP_REACH_DEVICE_PX: f32 = 0.5;

/// The transform from the walk's current frame to the device pixels of the
/// target being rasterized (none past a non-2D matrix), and whether the
/// rounding against those pixels is off for this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridMap {
  map: Option<euclid::default::Transform2D<f32>>,
  unrounded: bool,
}

impl GridMap {
  /// Whether rounding is off at this frame.
  pub(crate) fn unrounded(&self) -> bool {
    self.unrounded
  }

  /// This grid with rounding also off when `off` is set: a one-way switch,
  /// inherited by every frame derived from it.
  pub(crate) fn or_unrounded(self, off: bool) -> Self {
    Self { unrounded: self.unrounded || off, ..self }
  }
}

/// The grid of a raster at `scale` device pixels per frame unit.
pub(crate) fn at_scale(scale: f32) -> GridMap {
  GridMap { map: Some(euclid::default::Transform2D::scale(scale, scale)), unrounded: false }
}

/// The grid of a raster whose builder was prepared as `scale(scale)` then
/// `translate(by)`: frame point p lands on device pixel (p + by) * scale.
pub(crate) fn raster(scale: f32, by: Vector) -> GridMap {
  translate(&at_scale(scale), by)
}

/// `map` one op deeper: the walk applied `m` to the frame it is in.
pub(crate) fn through(map: &GridMap, m: &Matrix) -> GridMap {
  let inner = map.map.filter(|_| m.is_2d()).map(|cur| m.to_2d().then(&cur));
  GridMap { map: inner, ..*map }
}

/// `map` past a translation of the frame by `v`.
pub(crate) fn translate(map: &GridMap, v: Vector) -> GridMap {
  GridMap { map: map.map.map(|m| m.pre_translate(v)), ..*map }
}

/// The scale the chain applies at this frame: the longer axis of the map,
/// device pixels per frame unit; None without a map.
pub(crate) fn scale(map: &GridMap) -> Option<f32> {
  let m = map.map.as_ref()?;
  Some((m.m11 * m.m11 + m.m12 * m.m12).sqrt().max((m.m21 * m.m21 + m.m22 * m.m22).sqrt()))
}

// The map when boxes of this frame are rounded: the frame is axis-aligned
// with the grid (a translate and a positive scale only) and rounding is
// on; None otherwise, where nothing snaps.
fn axis_aligned(map: &GridMap) -> Option<&euclid::default::Transform2D<f32>> {
  if map.unrounded {
    return None;
  }
  let m = map.map.as_ref()?;
  (m.m12 == 0.0 && m.m21 == 0.0 && m.m11 > 0.0 && m.m22 > 0.0).then_some(m)
}

/// Whether anything is rounded at this frame: it is axis-aligned with the
/// grid and rounding is on.
pub(crate) fn rounds(map: &GridMap) -> bool {
  axis_aligned(map).is_some()
}

/// The offset, in the current frame, that moves `point` onto a whole
/// device pixel; None where nothing is rounded.
pub(crate) fn shift(map: &GridMap, point: Point) -> Option<Vector> {
  let m = axis_aligned(map)?;
  let device = m.transform_point(point);
  let residual = device.round() - device;
  Some(Vector::new(residual.x / m.m11, residual.y / m.m22))
}

/// A movement `v` of the current frame, rounded to whole device pixels per
/// axis: a group moved by it keeps the fractions of its children, so their
/// one rounding at the draw stays where it was. Zero stays zero. Unchanged
/// where nothing is rounded.
pub(crate) fn snap_move(map: &GridMap, v: Vector) -> Vector {
  let Some(m) = axis_aligned(map) else { return v };
  Vector::new((v.x * m.m11).round() / m.m11, (v.y * m.m22).round() / m.m22)
}

/// A box at `pos` of `size` in the current frame, snapped: its origin and
/// its far edges each moved to the nearest whole device pixel, so the box
/// ends where the next one starts. Unchanged where nothing is rounded.
pub(crate) fn snap_box(map: &GridMap, pos: Point, size: Size) -> (Point, Size) {
  let Some(m) = axis_aligned(map) else { return (pos, size) };
  let device = m.transform_point(pos);
  let near = device.round();
  let far = m.transform_point(pos + size.to_vector()).round();
  let origin = Point::new(pos.x + (near.x - device.x) / m.m11, pos.y + (near.y - device.y) / m.m22);
  let size = Size::new((far.x - near.x) / m.m11, (far.y - near.y) / m.m22);
  (origin, size)
}
