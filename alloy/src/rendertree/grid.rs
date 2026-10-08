// The device pixel grid (okf/done/pixel-snapped-paint-boxes.md): layout
// places nodes at fractional positions (a 16.98 px row puts its rows at
// 45.71, 62.69, 79.67), and a box painted there is resampled a fraction off
// on every edge - a 1 px border reads as two grey rows, a divider blurs, a
// snapshot's texture carries the blur to all of its content. The paint walk
// snaps instead, at paint time only, as browsers do: every axis-aligned
// box lands with its device origin on a whole pixel and its far edges on
// whole pixels too, so neighbours still share an edge without a seam, and
// content moves by under a pixel. Layout and hit testing never see the
// snapped boxes.
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
use crate::impellers::{Matrix, Point, Size};
use crate::rendertree::Vector;

/// The transform from the walk's current frame to the device pixels of the
/// target being rasterized; None past a non-2D matrix.
pub type GridMap = Option<euclid::default::Transform2D<f32>>;

/// The grid of a raster at `scale` device pixels per frame unit.
pub(crate) fn at_scale(scale: f32) -> GridMap {
  Some(euclid::default::Transform2D::scale(scale, scale))
}

/// The grid of a raster whose builder was prepared as `scale(scale)` then
/// `translate(by)`: frame point p lands on device pixel (p + by) * scale.
pub(crate) fn raster(scale: f32, by: Vector) -> GridMap {
  at_scale(scale).map(|m| m.pre_translate(by))
}

/// `map` one op deeper: the walk applied `m` to the frame it is in.
pub(crate) fn through(map: &GridMap, m: &Matrix) -> GridMap {
  let cur = (*map)?;
  if !m.is_2d() {
    return None;
  }
  Some(m.to_2d().then(&cur))
}

/// `map` past a translation of the frame by `v`.
pub(crate) fn translate(map: &GridMap, v: Vector) -> GridMap {
  map.map(|m| m.pre_translate(v))
}

/// The scale the chain applies at this frame: the longer axis of the map,
/// device pixels per frame unit; None without a map.
pub(crate) fn scale(map: &GridMap) -> Option<f32> {
  let m = map.as_ref()?;
  Some((m.m11 * m.m11 + m.m12 * m.m12).sqrt().max((m.m21 * m.m21 + m.m22 * m.m22).sqrt()))
}

// The map when the frame is axis-aligned with the grid (a translate and a
// positive scale only); None otherwise, where nothing snaps.
fn axis_aligned(map: &GridMap) -> Option<&euclid::default::Transform2D<f32>> {
  let m = map.as_ref()?;
  (m.m12 == 0.0 && m.m21 == 0.0 && m.m11 > 0.0 && m.m22 > 0.0).then_some(m)
}

/// The offset, in the current frame, that moves `point` onto a whole
/// device pixel; None when the frame is not axis-aligned with the grid.
pub(crate) fn shift(map: &GridMap, point: Point) -> Option<Vector> {
  let m = axis_aligned(map)?;
  let device = m.transform_point(point);
  let residual = device.round() - device;
  Some(Vector::new(residual.x / m.m11, residual.y / m.m22))
}

/// A box at `pos` of `size` in the current frame, snapped: its origin and
/// its far edges each moved to the nearest whole device pixel, so the box
/// ends where the next one starts. Unchanged when the frame is not
/// axis-aligned with the grid.
pub(crate) fn snap_box(map: &GridMap, pos: Point, size: Size) -> (Point, Size) {
  let Some(m) = axis_aligned(map) else { return (pos, size) };
  let device = m.transform_point(pos);
  let near = device.round();
  let far = m.transform_point(pos + size.to_vector()).round();
  let origin = Point::new(pos.x + (near.x - device.x) / m.m11, pos.y + (near.y - device.y) / m.m22);
  let size = Size::new((far.x - near.x) / m.m11, (far.y - near.y) / m.m22);
  (origin, size)
}
