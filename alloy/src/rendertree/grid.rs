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
//
// A stroke width is a width and not a position
// (okf/done/border-width-device-pixels.md): a 1 px stroke at 1.5x puts its
// inner edge on a half pixel however well its box is snapped. So a rect's stroke
// is floored to whole device pixels, at least one, as browsers snap a
// border width (CSS Values 4, "snap a length as a border width"), not
// rounded like an edge. It holds where the rounding of positions is off:
// a running track moves the box, not the width, and `pixelSnap={false}` is
// about positions.
use crate::renderer::{Matrix, Point, Size};
use crate::rendertree::Vector;

/// How far a rounded edge can sit from the fractional frame it was rounded
/// in, in device pixels: half a pixel, the most a rounding moves. What the
/// cull rect, stated in the fractional frame, grows by so it still holds
/// everything a snapped clip shows.
pub(crate) const SNAP_REACH_DEVICE_PX: f32 = 0.5;

/// How close below a whole device pixel a stroke width still counts as
/// that pixel, in device pixels: matrix noise (3.9999 for 4) must lose no
/// pixel to the floor.
const STROKE_FLOOR_EPSILON_DEVICE_PX: f32 = 1e-3;

/// The narrowest a floored stroke paints, in device pixels: a stroke above
/// zero and below one device pixel is one, never nothing.
const STROKE_MIN_DEVICE_PX: f32 = 1.0;

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

/// Whether this frame has one device width for a stroke: the map is there,
/// axis-aligned (no rotation, no skew) and the same finite, positive scale
/// on both axes. Reads the map itself and not `axis_aligned`, so it ignores
/// `unrounded`: a stroke width is floored under a running track and under
/// `pixelSnap={false}` too (okf/done/border-width-device-pixels.md).
pub(crate) fn floors_strokes(map: &GridMap) -> bool {
  map.map.as_ref().is_some_and(|m| m.m12 == 0.0 && m.m21 == 0.0 && m.m11.is_finite() && m.m11 > 0.0 && m.m11 == m.m22)
}

/// The width, in frame units, a stroke of `width` paints at: a whole number
/// of device pixels, `width` floored to them and at least one
/// (okf/done/border-width-device-pixels.md). `rest` is where the running
/// scale transitions take the frame's scale (BuildContext::scale_target, 1
/// with none): the floor is taken at the scale the stroke rests at, the
/// frame's scale times `rest`, so a zoom carries one width from start to
/// end. The minimum of one device pixel is not taken there: where the
/// floor at the rest scale is under one device pixel, the answer is the
/// floor at the frame's own scale, at least one. Taken at the rest scale,
/// a zoom out would widen the stroke at the scale it is drawn at (1 px at
/// 1x headed to 0.5 would paint 2 device pixels when the transition is
/// written, and for the whole hold of a delayed one). With `rest` 1 the
/// two are the same. None, the declared width painted, where the frame has
/// no single device width (`floors_strokes`), or where the rest scale is
/// not finite and positive (an infinite `rest`, a product that overflows
/// or underflows), so the answer is never NaN or infinite. A zero width
/// stays zero: it paints nothing (PaintState::strokes).
pub(crate) fn stroke_width(map: &GridMap, width: f32, rest: f32) -> Option<f32> {
  let own = map.map.as_ref().filter(|_| floors_strokes(map))?.m11;
  let rest_scale = own * rest;
  if !rest_scale.is_finite() || rest_scale <= 0.0 {
    return None;
  }
  if width <= 0.0 {
    return Some(width);
  }
  let at_rest = (width * rest_scale + STROKE_FLOOR_EPSILON_DEVICE_PX).floor();
  let painted = match at_rest >= STROKE_MIN_DEVICE_PX {
    true => at_rest / rest_scale,
    false => (width * own + STROKE_FLOOR_EPSILON_DEVICE_PX).floor().max(STROKE_MIN_DEVICE_PX) / own,
  };
  // A width too large for f32 at this scale: the declared one is painted.
  painted.is_finite().then_some(painted)
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
