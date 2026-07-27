use crate::impellers::{Point, Rect, Size};
use crate::rendertree::{fit_rects, TextureFit};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

fn parts(r: Rect) -> (f32, f32, f32, f32) {
  (r.origin.x, r.origin.y, r.size.width, r.size.height)
}

// A 200x100 source into a 100x100 box exercises every fit distinctly.
const SRC: (f32, f32, f32, f32) = (0.0, 0.0, 200.0, 100.0);
const DST: (f32, f32, f32, f32) = (10.0, 20.0, 100.0, 100.0);

fn run(fit: TextureFit) -> ((f32, f32, f32, f32), (f32, f32, f32, f32)) {
  let (s, d) = fit_rects(fit, rect(SRC.0, SRC.1, SRC.2, SRC.3), rect(DST.0, DST.1, DST.2, DST.3));
  (parts(s), parts(d))
}

#[test]
fn fill_passes_rects_through() {
  assert_eq!(run(TextureFit::Fill), (SRC, DST));
}

#[test]
fn cover_crops_source_centered() {
  // Scale = max(100/200, 100/100) = 1; visible source = 100x100, centered
  // horizontally in the 200-wide source; the destination box is filled.
  assert_eq!(run(TextureFit::Cover), ((50.0, 0.0, 100.0, 100.0), DST));
}

#[test]
fn contain_letterboxes_destination_centered() {
  // Scale = min(100/200, 100/100) = 0.5; drawn size = 100x50, centered
  // vertically in the box; the full source is shown.
  assert_eq!(run(TextureFit::Contain), (SRC, (10.0, 45.0, 100.0, 50.0)));
}

#[test]
fn none_draws_intrinsic_size_cropped() {
  // Scale 1: the 200-wide source is cropped to the 100-wide box, centered;
  // the 100-high axis fits exactly.
  assert_eq!(run(TextureFit::None), ((50.0, 0.0, 100.0, 100.0), DST));
}

#[test]
fn scale_down_acts_as_contain_when_source_is_larger() {
  assert_eq!(run(TextureFit::ScaleDown), run(TextureFit::Contain));
}

#[test]
fn scale_down_never_upscales() {
  // A 50x25 source in a 100x100 box: contain would scale by 2; scale-down
  // pins to 1 and centers at intrinsic size.
  let (s, d) = fit_rects(TextureFit::ScaleDown, rect(0.0, 0.0, 50.0, 25.0), rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(parts(s), (0.0, 0.0, 50.0, 25.0));
  assert_eq!(parts(d), (25.0, 37.5, 50.0, 25.0));
}

#[test]
fn none_centers_smaller_source_without_crop() {
  let (s, d) = fit_rects(TextureFit::None, rect(0.0, 0.0, 40.0, 20.0), rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(parts(s), (0.0, 0.0, 40.0, 20.0));
  assert_eq!(parts(d), (30.0, 40.0, 40.0, 20.0));
}

#[test]
fn cover_respects_source_crop_offset() {
  // A src_* crop (origin 20,10) stays the coordinate base for the sub-rect.
  let (s, d) = fit_rects(TextureFit::Cover, rect(20.0, 10.0, 200.0, 100.0), rect(0.0, 0.0, 100.0, 100.0));
  assert_eq!(parts(s), (70.0, 10.0, 100.0, 100.0));
  assert_eq!(parts(d), (0.0, 0.0, 100.0, 100.0));
}

#[test]
fn degenerate_sizes_pass_through() {
  let z = rect(0.0, 0.0, 0.0, 100.0);
  let d = rect(0.0, 0.0, 100.0, 100.0);
  assert_eq!((parts(z), parts(d)), {
    let (s, o) = fit_rects(TextureFit::Cover, z, d);
    (parts(s), parts(o))
  });
  let s2 = rect(0.0, 0.0, 100.0, 100.0);
  let z2 = rect(0.0, 0.0, 100.0, 0.0);
  assert_eq!((parts(s2), parts(z2)), {
    let (s, o) = fit_rects(TextureFit::Contain, s2, z2);
    (parts(s), parts(o))
  });
}

// The texture element is the one raster kind in the tree, and it reaches its
// paint through the same accessor the vector kinds do. Without this arm the
// property adapter never offers a paint prop to a texture and `blendMode`
// comes back as an unknown property.
#[test]
fn texture_exposes_its_paint() {
  use crate::impellers::BlendMode;
  use crate::rendertree::{Damage, ElementKind, Texture};

  let mut kind = ElementKind::Texture(Texture::default());
  let paint = kind.paint_mut().expect("texture kind exposes a paint");
  assert_eq!(paint.blend_mode, BlendMode::SourceOver);
  assert_eq!(paint.set_blend_mode(BlendMode::Plus), Damage::Paint);

  match &kind {
    ElementKind::Texture(tex) => assert_eq!(tex.paint.blend_mode, BlendMode::Plus),
    _ => panic!("kind changed"),
  }
}
