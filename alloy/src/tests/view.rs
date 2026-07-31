use crate::rendertree::{OriginCoord, View, WH};

#[test]
fn origin_defaults_to_center() {
  let v = View::default();
  let c = v.resolve_center(WH::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (100.0, 50.0));
}

#[test]
fn fraction_origin_tracks_size() {
  let mut v = View::default();
  v.set_origin_x(OriginCoord::Fraction(0.0));
  v.set_origin_y(OriginCoord::Fraction(1.0));
  // Left on x, bottom on y, resolved against the live extent.
  let c = v.resolve_center(WH::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (0.0, 100.0));
  // Same fractions, a different size: the pivot moves with the box.
  let c = v.resolve_center(WH::new(40.0, 60.0));
  assert_eq!((c.x, c.y), (0.0, 60.0));
}

#[test]
fn unset_axis_falls_back_to_center() {
  let mut v = View::default();
  // Only x is set; y must still default to the box center.
  v.set_origin_x(OriginCoord::Px(10.0));
  let c = v.resolve_center(WH::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (10.0, 50.0));
}

#[test]
fn pixel_origin_is_absolute() {
  let mut v = View::default();
  v.set_origin_x(OriginCoord::Px(20.0));
  v.set_origin_y(OriginCoord::Px(30.0));
  let c = v.resolve_center(WH::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (20.0, 30.0));
  // Unaffected by size, unlike a fraction.
  let c = v.resolve_center(WH::new(40.0, 60.0));
  assert_eq!((c.x, c.y), (20.0, 30.0));
}

// Maps a point through the matrix on the z = 0 plane (no perspective in these
// tests, so no divide needed).
fn map(m: &crate::impellers::Matrix, x: f32, y: f32) -> (f32, f32) {
  (x * m.m11 + y * m.m21 + m.m41, x * m.m12 + y * m.m22 + m.m42)
}

#[test]
fn view_box_fits_and_centers() {
  let mut v = View::default();
  v.set_view_box(100.0, 50.0);
  // Wide design in a square box: scale by the tighter axis (200/100 = 2),
  // centered vertically ((200 - 50*2) / 2 = 50).
  let m = v.paint_matrix(WH::new(200.0, 200.0));
  assert_eq!(map(&m, 0.0, 0.0), (0.0, 50.0));
  assert_eq!(map(&m, 100.0, 50.0), (200.0, 150.0));
}

#[test]
fn view_box_stays_out_of_box_matrix() {
  let mut v = View::default();
  v.set_view_box(100.0, 100.0);
  v.set_x(10.0);
  let size = WH::new(200.0, 200.0);
  // The full matrix carries fit and translate; the box matrix only the user
  // chain, so the view's own box is never inflated by the fit.
  assert_eq!(map(&v.paint_matrix(size), 100.0, 100.0), (210.0, 200.0));
  assert_eq!(map(&v.box_matrix(size), 200.0, 200.0), (210.0, 200.0));
}

#[test]
fn view_box_composes_inside_user_transforms() {
  let mut v = View::default();
  v.set_view_box(100.0, 100.0);
  v.set_scale_x(2.0);
  v.set_scale_y(2.0);
  let size = WH::new(100.0, 100.0);
  // Fit is identity here (design == box); the user scale doubles around the
  // box center: design center stays put, the design corner lands at the
  // doubled corner.
  let m = v.paint_matrix(size);
  assert_eq!(map(&m, 50.0, 50.0), (50.0, 50.0));
  assert_eq!(map(&m, 100.0, 100.0), (150.0, 150.0));
}
