use crate::rendertree::{Element, ElementKind, OriginCoord, Size, View};

#[test]
fn origin_defaults_to_center() {
  let v = View::default();
  let c = v.resolve_center(Size::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (100.0, 50.0));
}

#[test]
fn detached_origin_defaults_to_local_origin() {
  // Built the way the plugin builds one, so the flag rides the real path. The
  // size passed here is the INHERITED box (a d-view has none of its own); it
  // must not supply a center pivot.
  let el = Element::from_kind("d-view").expect("known kind");
  let ElementKind::View(v) = &el.kind else { panic!("d-view must be a View") };
  let c = v.resolve_center(Size::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (0.0, 0.0));
}

#[test]
fn detached_scale_pivots_at_local_origin() {
  // The probe that motivated the default: scale on a d-view leaves content
  // authored at (0,0) in place instead of pulling it toward the inherited
  // box's center.
  let mut el = Element::from_kind("d-view").expect("known kind");
  let ElementKind::View(v) = &mut el.kind else { panic!("d-view must be a View") };
  v.set_scale_x(Some(0.5));
  v.set_scale_y(Some(0.5));
  let m = v.paint_matrix(Size::new(1692.0, 1128.0));
  assert_eq!(map(&m, 0.0, 0.0), (0.0, 0.0));
  assert_eq!(map(&m, 200.0, 100.0), (100.0, 50.0));
}

#[test]
fn detached_explicit_origin_still_resolves_against_inherited_size() {
  // Only the unset default changes: an explicit origin keeps its documented
  // meaning, fractions resolving against the box the view inherits.
  let mut el = Element::from_kind("d-view").expect("known kind");
  let ElementKind::View(v) = &mut el.kind else { panic!("d-view must be a View") };
  v.set_origin_x(Some(OriginCoord::Fraction(0.5)));
  v.set_origin_y(Some(OriginCoord::Px(30.0)));
  let c = v.resolve_center(Size::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (100.0, 30.0));
}

#[test]
fn fraction_origin_tracks_size() {
  let mut v = View::default();
  v.set_origin_x(Some(OriginCoord::Fraction(0.0)));
  v.set_origin_y(Some(OriginCoord::Fraction(1.0)));
  // Left on x, bottom on y, resolved against the live extent.
  let c = v.resolve_center(Size::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (0.0, 100.0));
  // Same fractions, a different size: the pivot moves with the box.
  let c = v.resolve_center(Size::new(40.0, 60.0));
  assert_eq!((c.x, c.y), (0.0, 60.0));
}

#[test]
fn unset_axis_falls_back_to_center() {
  let mut v = View::default();
  // Only x is set; y must still default to the box center.
  v.set_origin_x(Some(OriginCoord::Px(10.0)));
  let c = v.resolve_center(Size::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (10.0, 50.0));
}

#[test]
fn pixel_origin_is_absolute() {
  let mut v = View::default();
  v.set_origin_x(Some(OriginCoord::Px(20.0)));
  v.set_origin_y(Some(OriginCoord::Px(30.0)));
  let c = v.resolve_center(Size::new(200.0, 100.0));
  assert_eq!((c.x, c.y), (20.0, 30.0));
  // Unaffected by size, unlike a fraction.
  let c = v.resolve_center(Size::new(40.0, 60.0));
  assert_eq!((c.x, c.y), (20.0, 30.0));
}

// Maps a point through the matrix on the z = 0 plane (no perspective in these
// tests, so no divide needed).
fn map(m: &crate::impellers::Matrix, x: f32, y: f32) -> (f32, f32) {
  (x * m.m11 + y * m.m21 + m.m41, x * m.m12 + y * m.m22 + m.m42)
}

#[test]
fn design_size_fits_and_centers() {
  let mut v = View::default();
  v.set_design_size(Some((100.0, 50.0)));
  // Wide design in a square box: scale by the tighter axis (200/100 = 2),
  // centered vertically ((200 - 50*2) / 2 = 50).
  let m = v.paint_matrix(Size::new(200.0, 200.0));
  assert_eq!(map(&m, 0.0, 0.0), (0.0, 50.0));
  assert_eq!(map(&m, 100.0, 50.0), (200.0, 150.0));
}

#[test]
fn design_size_stays_out_of_box_matrix() {
  let mut v = View::default();
  v.set_design_size(Some((100.0, 100.0)));
  v.set_x(Some(10.0));
  let size = Size::new(200.0, 200.0);
  // The full matrix carries fit and translate; the box matrix only the user
  // chain, so the view's own box is never inflated by the fit.
  assert_eq!(map(&v.paint_matrix(size), 100.0, 100.0), (210.0, 200.0));
  assert_eq!(map(&v.box_matrix(size), 200.0, 200.0), (210.0, 200.0));
}

#[test]
fn design_size_composes_inside_user_transforms() {
  let mut v = View::default();
  v.set_design_size(Some((100.0, 100.0)));
  v.set_scale_x(Some(2.0));
  v.set_scale_y(Some(2.0));
  let size = Size::new(100.0, 100.0);
  // Fit is identity here (design == box); the user scale doubles around the
  // box center: design center stays put, the design corner lands at the
  // doubled corner.
  let m = v.paint_matrix(size);
  assert_eq!(map(&m, 50.0, 50.0), (50.0, 50.0));
  assert_eq!(map(&m, 100.0, 100.0), (150.0, 150.0));
}

#[test]
fn none_resets_transform_props_to_unset() {
  // A cleared JS binding writes None through the setters: the view returns
  // to its unset state, including the cheap translation-only bounds path
  // (needs_matrix) that Some(1.0) could not restore.
  let mut v = View::default();
  v.set_scale_x(Some(2.0));
  v.set_scale_y(Some(2.0));
  v.set_rotate(Some(1.0));
  v.set_design_size(Some((100.0, 50.0)));
  assert!(v.needs_matrix());
  v.set_scale_x(None);
  v.set_scale_y(None);
  v.set_rotate(None);
  v.set_design_size(None);
  assert!(!v.needs_matrix());

  // x/y reset the translate component; the identity map comes back.
  v.set_x(Some(30.0));
  v.set_y(Some(40.0));
  v.set_x(None);
  v.set_y(None);
  let size = Size::new(100.0, 100.0);
  assert_eq!(map(&v.paint_matrix(size), 10.0, 20.0), (10.0, 20.0));

  // Opacity clears to opaque (None), not to Some(1.0).
  v.set_opacity(Some(0.5));
  v.set_opacity(None);
  assert_eq!(v.opacity, None);
}
