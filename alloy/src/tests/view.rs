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
