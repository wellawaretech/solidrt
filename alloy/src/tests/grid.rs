//! The device pixel grid (rendertree/grid.rs): what the paint-time snap does
//! to a box under a translate, a scale, a raster root, and a rotation.
use crate::impellers::{Matrix, Point, Size};
use crate::rendertree::grid::{at_scale, raster, shift, snap_box, through};
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
  assert!(through(&at_scale(1.0), &tilt).is_none());
}
