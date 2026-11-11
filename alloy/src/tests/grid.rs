//! The device pixel grid (rendertree/grid.rs): what the paint-time snap does
//! to a box under a translate, a scale, a raster root, and a rotation, and
//! the device-pixel floor of a stroke width.
use crate::renderer::{Matrix, Point, Size};
use crate::rendertree::grid::{
  at_scale, floors_strokes, raster, rounds, scale, shift, snap_box, snap_move, stroke_width, through, translate,
};
use crate::rendertree::Vector;

fn close(a: f32, b: f32) -> bool {
  (a - b).abs() < 1e-4
}

// The coverage probe's rows at 1x: a 16.98 px row at 45.71 lands on 46
// and ends on 63, one row taller than its layout, so the next row starts
// where it ends.
#[test]
fn snaps_origin_and_far_edges_at_1x() {
  let grid = at_scale(1.0);
  let (pos, size) = snap_box(&grid, Point::new(0.3, 45.71), Size::new(100.4, 16.98));
  assert!(close(pos.x, 0.0) && close(pos.y, 46.0), "{pos:?}");
  assert!(close(size.width, 101.0) && close(size.height, 17.0), "{size:?}");
  let (next, _) = snap_box(&grid, Point::new(0.3, 45.71 + 16.98), Size::new(100.4, 16.98));
  assert!(close(next.y, pos.y + size.height), "rows tile: {next:?} after {pos:?} + {size:?}");
}

// At 1.5x the grid is the device's: 16.98 logical is 25.47 device, which
// snaps to pixel 25, two thirds of a logical pixel.
#[test]
fn snaps_to_device_pixels_at_1_5x() {
  let grid = at_scale(1.5);
  let (pos, size) = snap_box(&grid, Point::new(0.0, 16.98), Size::new(400.0, 16.98));
  assert!(close(pos.y, 25.0 / 1.5), "{pos:?}");
  assert!(close(size.height, (51.0 - 25.0) / 1.5), "{size:?}");
  assert!(close(size.width, 400.0), "{size:?}");
}

// The shift is stated in the current frame: under a 2x ancestor scale a
// 0.3 frame offset is 0.6 device, snapped to 1, a 0.2 frame shift.
#[test]
fn shift_is_in_frame_units_under_a_scale() {
  let grid = through(&at_scale(1.0), &Matrix::scale(2.0, 2.0, 1.0));
  let by = shift(&grid, Point::new(0.3, 0.0)).expect("axis-aligned");
  assert!(close(by.x, 0.2) && close(by.y, 0.0), "{by:?}");
}

// A raster prepared as scale(2) then translate(3, 3) puts frame point p at
// device (p + 3) * 2; the snap is against that texture's pixels.
#[test]
fn raster_root_has_its_own_grid() {
  let grid = raster(2.0, Vector::new(3.0, 3.0));
  let by = shift(&grid, Point::new(0.2, 0.0)).expect("axis-aligned");
  // device 6.4 rounds to 6: back by 0.2 frame units.
  assert!(close(by.x, -0.2) && close(by.y, 0.0), "{by:?}");
}

// A rotation leaves no grid to land on: nothing moves, and a 3d matrix
// drops the map entirely.
#[test]
fn rotation_and_3d_do_not_snap() {
  let rotated = through(&at_scale(1.0), &Matrix::new_2d(0.0, 1.0, -1.0, 0.0, 0.0, 0.0));
  assert!(shift(&rotated, Point::new(0.3, 0.3)).is_none());
  let (pos, size) = snap_box(&rotated, Point::new(0.3, 45.71), Size::new(100.4, 16.98));
  assert!(close(pos.y, 45.71) && close(size.height, 16.98), "{pos:?} {size:?}");
  let (s, c) = 0.5f32.sin_cos();
  #[rustfmt::skip]
  let tilt = Matrix::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, c,   s,   0.0,
    0.0, -s,  c,   0.0,
    0.0, 0.0, 0.0, 1.0,
  );
  assert!(scale(&through(&at_scale(1.0), &tilt)).is_none());
}

// A movement rounds to whole device pixels per axis, in frame units, and a
// zero movement stays zero: at 1.5x a 0.3 px move is 0.45 device, no move
// at all, and a 10.4 px one is 15.6 device, 16, so 10.667 px.
#[test]
fn a_movement_rounds_to_whole_device_pixels() {
  let grid = at_scale(1.5);
  let by = snap_move(&grid, Vector::new(0.3, 10.4));
  assert!(close(by.x, 0.0) && close(by.y, 16.0 / 1.5), "{by:?}");
  assert_eq!(snap_move(&grid, Vector::zero()), Vector::zero());
  // Where the frame sits does not matter: a movement is not a position.
  let moved = translate(&grid, Vector::new(0.37, 0.61));
  assert_eq!(snap_move(&moved, Vector::new(0.3, 10.4)), by);
  // Under a rotation there is no grid to move along.
  let rotated = through(&grid, &Matrix::new_2d(0.0, 1.0, -1.0, 0.0, 0.0, 0.0));
  assert_eq!(snap_move(&rotated, Vector::new(0.3, 10.4)), Vector::new(0.3, 10.4));
}

// A box is rounded once, on its absolute position: a parent at 10.4 with a
// child at +0.3 lands the child on 11 (10.7 rounded), where rounding the
// parent first and the child on top of it gave 10.
#[test]
fn a_nested_box_rounds_once() {
  let parent = translate(&at_scale(1.0), Vector::new(10.4, 0.0));
  let (pos, _) = snap_box(&parent, Point::new(0.3, 0.0), Size::new(5.0, 5.0));
  assert!(close(10.4 + pos.x, 11.0), "{pos:?}");
}

// With the rounding off (a running track, pixelSnap={false}) nothing moves:
// no box, no point, no movement. The flag is one-way and every frame
// derived from the grid keeps it; the scale still reads.
#[test]
fn an_unrounded_grid_rounds_nothing() {
  let grid = at_scale(1.5).or_unrounded(true);
  assert!(grid.unrounded() && !rounds(&grid) && rounds(&at_scale(1.5)));
  assert!(shift(&grid, Point::new(0.3, 0.3)).is_none());
  let (pos, size) = snap_box(&grid, Point::new(0.3, 45.71), Size::new(100.4, 16.98));
  assert!(close(pos.y, 45.71) && close(size.height, 16.98), "{pos:?} {size:?}");
  assert_eq!(snap_move(&grid, Vector::new(0.3, 10.4)), Vector::new(0.3, 10.4));
  let deeper = through(&translate(&grid, Vector::new(3.0, 3.0)), &Matrix::scale(2.0, 2.0, 1.0));
  assert!(deeper.unrounded() && deeper.or_unrounded(false).unrounded());
  assert!(close(scale(&deeper).expect("a 2d chain"), 3.0));
  // A raster root starts over: its grid rounds.
  assert!(rounds(&raster(2.0, Vector::new(3.0, 3.0))));
}

// A stroke width is floored to whole device pixels, at least one
// (okf/done/border-width-device-pixels.md): 1 px at 1.5x is 1.5 device
// pixels, floored to 1; 2 px at 1.5x is 3; 0.5 px at 1x is half a device
// pixel, raised to the minimum of 1.
#[test]
fn a_stroke_width_floors_to_whole_device_pixels() {
  let grid = at_scale(1.5);
  assert!(close(stroke_width(&grid, 1.0, 1.0).expect("uniform"), 1.0 / 1.5));
  assert!(close(stroke_width(&grid, 2.0, 1.0).expect("uniform"), 3.0 / 1.5));
  assert!(close(stroke_width(&at_scale(1.0), 0.5, 1.0).expect("uniform"), 1.0));
}

// Matrix noise loses no pixel: a width a hair under 4 device pixels is 4.
#[test]
fn a_stroke_width_a_hair_under_a_pixel_keeps_it() {
  let grid = at_scale(1.5);
  let width = (4.0 - 1e-4) / 1.5;
  assert!(close(stroke_width(&grid, width, 1.0).expect("uniform"), 4.0 / 1.5));
}

// The floor is taken at the scale the stroke rests at: 1 px under a 1.5x
// grid headed to twice that is floored at 3x, 3 device pixels at rest,
// one frame unit.
#[test]
fn a_stroke_width_floors_at_the_rest_scale() {
  let width = stroke_width(&at_scale(1.5), 1.0, 2.0).expect("uniform");
  assert!(close(width, 1.0), "{width}");
}

// A zero width paints nothing and stays zero.
#[test]
fn a_zero_stroke_width_stays_zero() {
  assert_eq!(stroke_width(&at_scale(1.5), 0.0, 1.0), Some(0.0));
}

// No single device width: a rotation, a scale on one axis, a 3d matrix.
#[test]
fn a_stroke_width_is_not_floored_without_a_uniform_grid() {
  let rotated = through(&at_scale(1.5), &Matrix::new_2d(0.0, 1.0, -1.0, 0.0, 0.0, 0.0));
  assert_eq!(stroke_width(&rotated, 1.0, 1.0), None);
  let stretched = through(&at_scale(1.0), &Matrix::scale(2.0, 1.5, 1.0));
  assert_eq!(stroke_width(&stretched, 1.0, 1.0), None);
  let (s, c) = 0.5f32.sin_cos();
  #[rustfmt::skip]
  let tilt = Matrix::new(
    1.0, 0.0, 0.0, 0.0,
    0.0, c,   s,   0.0,
    0.0, -s,  c,   0.0,
    0.0, 0.0, 0.0, 1.0,
  );
  assert_eq!(stroke_width(&through(&at_scale(1.0), &tilt), 1.0, 1.0), None);
}

// A width is not a position: the floor holds with the rounding off (a
// running track, pixelSnap={false}).
#[test]
fn a_stroke_width_floors_with_the_rounding_off() {
  let grid = at_scale(1.5);
  let translated = translate(&grid, Vector::new(0.37, 0.61));
  for width in [0.5, 1.0, 2.0] {
    let rounded = stroke_width(&translated, width, 1.0);
    assert!(rounded.is_some());
    assert_eq!(stroke_width(&translated.or_unrounded(true), width, 1.0), rounded);
  }
}

// The minimum of one device pixel is not taken at the rest scale: where
// the rest scale has no whole width to give, the floor is the frame's own,
// so a zoom out never widens the stroke at the scale it is drawn at.
#[test]
fn a_zoom_out_does_not_widen_the_stroke_at_its_own_scale() {
  // 1 px at 1x headed to 0.5: one device pixel now, not two.
  assert!(close(stroke_width(&at_scale(1.0), 1.0, 0.5).expect("uniform"), 1.0));
  // 1 px at 1.5x headed to a quarter: the floor at 1.5x, one device pixel.
  assert!(close(stroke_width(&at_scale(1.5), 1.0, 0.25).expect("uniform"), 1.0 / 1.5));
  // 3 px at 1x headed to a quarter is 0.75 device pixels at rest: 3 now.
  assert!(close(stroke_width(&at_scale(1.0), 3.0, 0.25).expect("uniform"), 3.0));
  // A whole width at rest is taken there: 1 px at 0.5x headed to 4x is 2
  // device pixels at rest, one frame unit.
  assert!(close(stroke_width(&at_scale(0.5), 1.0, 4.0).expect("uniform"), 2.0 / (0.5 * 4.0)));
}

// A scale that is not finite and positive answers None, never NaN or an
// infinite width: an infinite grid, a product that underflows to zero and
// one that overflows.
#[test]
fn a_stroke_width_is_not_floored_at_a_degenerate_scale() {
  assert!(!floors_strokes(&at_scale(f32::INFINITY)));
  assert_eq!(stroke_width(&at_scale(f32::INFINITY), 1.0, 1.0), None);
  assert_eq!(stroke_width(&at_scale(1e-30), 1.0, 1e-30), None);
  assert_eq!(stroke_width(&at_scale(1e30), 1.0, 1e30), None);
  assert_eq!(stroke_width(&at_scale(1.5), 1.0, f32::INFINITY), None);
  assert_eq!(stroke_width(&at_scale(1.5), 1.0, f32::NAN), None);
}

// The predicate a recording stores: one device width for a stroke, with
// the rounding of positions on or off.
#[test]
fn floors_strokes_answers_for_an_upright_uniform_frame() {
  assert!(floors_strokes(&at_scale(1.5)));
  assert!(floors_strokes(&at_scale(1.5).or_unrounded(true)));
  assert!(!floors_strokes(&through(&at_scale(1.5), &Matrix::new_2d(0.0, 1.0, -1.0, 0.0, 0.0, 0.0))));
  assert!(!floors_strokes(&through(&at_scale(1.0), &Matrix::scale(1.0, 0.5, 1.0))));
}
