//! Draws under a perspective transform (stage 5, step 1 of
//! okf/plans/own-renderer.md), on the real GL path: every quad program
//! placed in homogeneous coordinates under its instance's projective row,
//! gradients evaluated on the instance in the quad's local space, and the
//! path meshes placed in homogeneous coordinates under their map as a
//! uniform, with their gradient evaluated in the path's space.
//!
//! Every case but the last draws under the view's card flip about the
//! target's center, as View::compose_user composes it: rotate_y by
//! FLIP_DEGREES, then the perspective foreshorten at PERSPECTIVE. The
//! expected pixels come from the same matrix on the CPU (a local point
//! through `transform_point3d` at z = 0, divided by w; a device pixel's
//! center back to the local plane through the inverse of the plane's map).
//! Verified, each case a list of its own on a clear target:
//!   1. a rect fill: a pixel inside the projected quad has the color, one
//!      past its projected edge is clear;
//!   2. the same as a rounded rect: the projected corner is clear, the
//!      projected center has the color;
//!   3. a linear gradient rect, red to blue left to right: at the projected
//!      point of local (w / 4, h / 2) the color is within two levels of the
//!      gradient at that pixel's local point (t near 0.25);
//!   4. a texture quad from a 2x2 checker, nearest: the four projected cell
//!      centers read the four colors;
//!   5. a glyph run of two quads over a 4x4 white atlas: the projected
//!      center of each quad is white;
//!   6. a dashed path (a straight line dashed DASH on DASH off, the dashes
//!      built as subpaths here, stroked through `draw_path`): the projected
//!      point half a dash along is painted, the one in the first gap clear;
//!   7. a gradient stroke (a path stroked with a linear gradient along it):
//!      the color at the projected mid point within two levels of the
//!      gradient there (t near 0.5);
//!   8. without perspective, an image source rotated 45 degrees on a rect:
//!      the pixel at each of the image's four cell centers reads the texel
//!      the inverse map gives.
//!
//! And through `Context::raster_counters`: a solid rect then a gradient
//! rect draw in two draw calls, two gradient rects in one.
//!
//! Run: cargo run -p alloy --example transform_cases

use alloy::renderer::prelude::*;
use alloy::renderer::{
  Color, ColorSource, DisplayList, DisplayListBuilder, DrawStyle, FillType, ISize, Matrix, Paint, Path, PathBuilder,
  Point, Rect, RoundingRadii, Size, StrokeCap, Texture, TextureSampling, TileMode,
};
use alloy::{Context, CoveragePolicy, GlyphQuad, SamplerState, TextureFormat};
use std::sync::Arc;

// The target's edge in pixels.
const SIZE: u32 = 256;
// The largest per-channel difference from an expected color: the 8-bit
// ramp and its bilinear lookup.
const TOLERANCE: f32 = 2.0;
// The card flip's angle and the perspective distance, in pixels.
const FLIP_DEGREES: f32 = 60.0;
const PERSPECTIVE: f32 = 800.0;
// The rect the quad cases draw, centered on the target.
const RECT_W: f32 = 200.0;
const RECT_H: f32 = 120.0;
// Case 1: how far past the projected quad's right edge the clear probe sits.
const OUTSIDE_PROBE: f32 = 4.0;
// Case 2: the corner radius, and how far in from the local corner its
// probe sits (outside the rounding: the corner circle's center is RADIUS
// in on both axes).
const RADIUS: f32 = 24.0;
const CORNER_PROBE: f32 = 3.0;
// Case 3: the local point's fraction of the rect's width.
const GRADIENT_PROBE: f32 = 0.25;
// Case 4: the checker's edge in texels.
const CHECKER: u32 = 2;
// Case 5: the atlas's edge, and each glyph quad's edge and the gap between
// the two quads, local pixels.
const ATLAS: u32 = 4;
const GLYPH: f32 = 40.0;
const GLYPH_GAP: f32 = 60.0;
// Case 6: the dashed line's length, its dash and gap length, and its
// stroke width.
const DASHED_LENGTH: f32 = 160.0;
const DASH: f32 = 20.0;
const DASH_WIDTH: f32 = 6.0;
// Case 7: the gradient stroke's width.
const STROKE_WIDTH: f32 = 12.0;
// Case 8: the image's rotation and the local pixels per texel.
const IMAGE_DEGREES: f32 = 45.0;
const IMAGE_SCALE: f32 = 40.0;
// The most a level-valued channel reads: 8-bit.
const LEVELS: f32 = 255.0;
// A pixel nothing drew.
const CLEAR: [u8; 4] = [0, 0, 0, 0];

const RED: Color = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
const GREEN: Color = Color::new_srgba(0.0, 1.0, 0.0, 1.0);
const BLUE: Color = Color::new_srgba(0.0, 0.0, 1.0, 1.0);
// The checker's four texels, row by row, rgba8.
const CHECKER_TEXELS: [[u8; 4]; 4] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];
const WHITE_TEXEL: [u8; 4] = [255, 255, 255, 255];

fn center() -> f32 {
  SIZE as f32 / 2.0
}

// The rect the quad cases draw.
fn card() -> Rect {
  let c = center();
  Rect::new(Point::new(c - RECT_W / 2.0, c - RECT_H / 2.0), Size::new(RECT_W, RECT_H))
}

// The card flip about the target's center, in View::compose_user's order
// (alloy/src/rendertree/kinds/view.rs).
fn flip() -> Matrix {
  let c = center();
  let (s, co) = FLIP_DEGREES.to_radians().sin_cos();
  #[rustfmt::skip]
  let ry = Matrix::new(
    co,  0.0, -s,  0.0,
    0.0, 1.0, 0.0, 0.0,
    s,   0.0, co,  0.0,
    0.0, 0.0, 0.0, 1.0,
  );
  Matrix::translation(-c, -c, 0.0)
    .then(&ry)
    .then(&Matrix::perspective(PERSPECTIVE))
    .then(&Matrix::translation(c, c, 0.0))
}

// The device point `m` puts the local point (x, y) on: z = 0, divided by w.
fn project(m: &Matrix, x: f32, y: f32) -> (f32, f32) {
  let p = m.transform_point3d(euclid::default::Point3D::new(x, y, 0.0)).expect("the point is in front of the eye");
  (p.x, p.y)
}

// The local point of the z = 0 plane that `m` puts on the device point
// (x, y): the plane's map x' = (x a + y b + c) / w, y' = (x d + y e + f) / w,
// w = x g + y h + i solved for (x, y).
fn unproject(m: &Matrix, x: f32, y: f32) -> (f32, f32) {
  let (a, b, c) = (m.m11, m.m21, m.m41);
  let (d, e, f) = (m.m12, m.m22, m.m42);
  let (g, h, i) = (m.m14, m.m24, m.m44);
  let (r0, r1, r2) = (a - x * g, b - x * h, x * i - c);
  let (s0, s1, s2) = (d - y * g, e - y * h, y * i - f);
  let det = r0 * s1 - r1 * s0;
  ((r2 * s1 - r1 * s2) / det, (r0 * s2 - r2 * s0) / det)
}

// The pixel the device point (x, y) lies in.
fn pixel_of(p: (f32, f32)) -> (u32, u32) {
  (p.0.floor() as u32, p.1.floor() as u32)
}

// The center of the pixel (x, y), in device pixels.
fn pixel_center(x: u32, y: u32) -> (f32, f32) {
  (x as f32 + 0.5, y as f32 + 0.5)
}

fn render(ctx: &Context, list: &DisplayList) -> Vec<u8> {
  let texture = ctx.render_display_list_to_texture(list, SIZE, SIZE, true).expect("rasterize the list");
  ctx.read_texture(&texture, SIZE, SIZE).expect("read the target back")
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
  let i = ((y * SIZE + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn save(name: &str, pixels: &[u8]) {
  let path = format!("/tmp/transform_cases_{name}.png");
  image::save_buffer(&path, pixels, SIZE, SIZE, image::ColorType::Rgba8).expect("save png");
  println!("saved {path}");
}

fn list(build: impl FnOnce(&mut DisplayListBuilder)) -> DisplayList {
  let mut b = DisplayListBuilder::new(None);
  build(&mut b);
  b.build().expect("build the list")
}

fn solid(color: Color) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(color);
  paint
}

// `color`, opaque, in levels.
fn levels(color: Color) -> [u8; 4] {
  let l = |v: f32| (v * LEVELS).round() as u8;
  [l(color.red), l(color.green), l(color.blue), l(color.alpha)]
}

// The red-to-blue gradient from x `from` to x `to` at `x`, opaque, in
// levels: the ramp's straight-alpha mix.
fn gradient_at(x: f32, from: f32, to: f32) -> [f32; 4] {
  let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
  let m = |a: f32, b: f32| (a + (b - a) * t) * LEVELS;
  [m(RED.red, BLUE.red), m(RED.green, BLUE.green), m(RED.blue, BLUE.blue), LEVELS]
}

// A paint with the red-to-blue gradient along x from `from` to `to`.
fn gradient_paint(from: f32, to: f32) -> Paint {
  let source = ColorSource::linear_gradient(
    Point::new(from, 0.0),
    Point::new(to, 0.0),
    &[RED, BLUE],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  );
  let mut paint = solid(Color::new_srgba(0.0, 0.0, 0.0, 1.0));
  paint.set_color_source(&source);
  paint
}

fn expect_color(case: &str, pixels: &[u8], at: (u32, u32), want: [u8; 4]) {
  let got = pixel(pixels, at.0, at.1);
  let close = got.iter().zip(want.iter()).all(|(g, w)| (*g as f32 - *w as f32).abs() <= TOLERANCE);
  assert!(close, "{case}: pixel {at:?} is {got:?}, want {want:?}");
}

fn expect_clear(case: &str, pixels: &[u8], at: (u32, u32)) {
  let got = pixel(pixels, at.0, at.1);
  assert_eq!(got, CLEAR, "{case}: pixel {at:?} is {got:?}, want clear");
}

fn expect_painted(case: &str, pixels: &[u8], at: (u32, u32)) {
  let got = pixel(pixels, at.0, at.1);
  assert!(got[3] > 0, "{case}: pixel {at:?} is clear, want painted");
}

// Assert the pixel `at` is the gradient from `from` to `to` at that pixel
// center's local x; returns the largest channel difference.
fn expect_gradient(case: &str, pixels: &[u8], m: &Matrix, at: (u32, u32), from: f32, to: f32) -> f32 {
  let (cx, cy) = pixel_center(at.0, at.1);
  let local = unproject(m, cx, cy);
  let want = gradient_at(local.0, from, to);
  let got = pixel(pixels, at.0, at.1);
  let diff = got.iter().zip(want.iter()).map(|(g, w)| (*g as f32 - w).abs()).fold(0.0, f32::max);
  assert!(diff <= TOLERANCE, "{case}: pixel {at:?} is {got:?}, want {want:?} (local x {})", local.0);
  diff
}

// A texture of `edge` x `edge` texels from `texels`, rgba8, row by row.
fn texture(ctx: &Context, edge: u32, texels: &[[u8; 4]], label: &str) -> Texture {
  let pixels: Vec<u8> = texels.iter().flatten().copied().collect();
  let id = ctx
    .create_texture_from_pixels(edge, edge, &pixels, SamplerState::default(), TextureFormat::Rgba8, Some(label.into()))
    .expect("create the texture");
  ctx.textures.get(id).and_then(|e| e.handle.clone()).expect("the texture has a renderer handle")
}

// The straight line of the path cases, from `start` local x over `length`
// at the target's middle row, as subpaths of `pieces` (start, end) ranges
// along it.
fn line_path(start: f32, pieces: &[(f32, f32)]) -> Path {
  let y = center();
  let mut b = PathBuilder::default();
  for (from, to) in pieces {
    b.move_to(Point::new(start + from, y)).line_to(Point::new(start + to, y));
  }
  b.build(FillType::NonZero)
}

// The renderer draw calls `draw` issues, from the raster counters.
fn draw_calls(ctx: &Context, draw: impl FnOnce()) -> u64 {
  let before = ctx.raster_counters().renderer_draw_calls;
  draw();
  ctx.raster_counters().renderer_draw_calls - before
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "transform cases probe", ISize::new(400, 300), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let m = flip();
    let r = card();
    let (x0, y0) = (r.min_x(), r.min_y());
    let c = center();

    // 1. A rect fill.
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_rect(&r, &solid(GREEN));
      }),
    );
    save("rect", &pixels);
    let corners = [(x0, y0), (r.max_x(), y0), (x0, r.max_y()), (r.max_x(), r.max_y())];
    let right = corners.iter().map(|(x, y)| project(&m, *x, *y).0).fold(f32::MIN, f32::max);
    expect_color("rect", &pixels, pixel_of(project(&m, c, c)), levels(GREEN));
    expect_color("rect", &pixels, pixel_of(project(&m, x0 + RECT_W * 0.9, c)), levels(GREEN));
    expect_clear("rect", &pixels, pixel_of((right + OUTSIDE_PROBE, c)));
    println!("ok: a rect under perspective fills its projected quad and nothing past its projected edge");

    // 2. A rounded rect.
    let radii = RoundingRadii::circular([RADIUS; 4]);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_rounded_rect(&r, &radii, &solid(GREEN));
      }),
    );
    save("rounded", &pixels);
    expect_clear("rounded", &pixels, pixel_of(project(&m, x0 + CORNER_PROBE, y0 + CORNER_PROBE)));
    expect_color("rounded", &pixels, pixel_of(project(&m, c, c)), levels(GREEN));
    println!("ok: a rounded rect under perspective is clear at its projected corner, filled at its center");

    // 3. A linear gradient rect, left to right.
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_rect(&r, &gradient_paint(x0, r.max_x()));
      }),
    );
    save("gradient", &pixels);
    let at = pixel_of(project(&m, x0 + RECT_W * GRADIENT_PROBE, c));
    let worst = expect_gradient("gradient", &pixels, &m, at, x0, r.max_x());
    println!("ok: a gradient rect under perspective reads its local t (largest channel difference {worst:.2})");

    // 4. A texture quad from a 2x2 checker, nearest.
    let checker = texture(&ctx, CHECKER, &CHECKER_TEXELS, "checker");
    let edge = CHECKER as f32;
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_texture_rect(
          &checker,
          &Rect::new(Point::zero(), Size::new(edge, edge)),
          &r,
          TextureSampling::Nearest,
          None,
        );
      }),
    );
    save("texture", &pixels);
    for (i, texel) in CHECKER_TEXELS.iter().enumerate() {
      let (cx, cy) = ((i as u32 % CHECKER) as f32 + 0.5, (i as u32 / CHECKER) as f32 + 0.5);
      let local = (x0 + cx / edge * RECT_W, y0 + cy / edge * RECT_H);
      expect_color("texture", &pixels, pixel_of(project(&m, local.0, local.1)), *texel);
    }
    println!("ok: a texture quad under perspective reads its four cells at their projected centers");

    // 5. A glyph run of two quads over a white atlas.
    let atlas = texture(&ctx, ATLAS, &[WHITE_TEXEL; (ATLAS * ATLAS) as usize], "atlas");
    let side = ATLAS as f32;
    let first = c - (GLYPH + GLYPH_GAP / 2.0);
    let second = c + GLYPH_GAP / 2.0;
    let top = c - GLYPH / 2.0;
    let quads: Arc<[GlyphQuad]> = Arc::from([
      GlyphQuad { dst: [first, top, GLYPH, GLYPH], src: [0.0, 0.0, side, side] },
      GlyphQuad { dst: [second, top, GLYPH, GLYPH], src: [0.0, 0.0, side, side] },
    ]);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_glyphs(&atlas, &quads, [1.0; 4], CoveragePolicy::default(), None);
      }),
    );
    save("glyphs", &pixels);
    for left in [first, second] {
      expect_color("glyphs", &pixels, pixel_of(project(&m, left + GLYPH / 2.0, c)), WHITE_TEXEL);
    }
    println!("ok: a glyph run under perspective is white at each quad's projected center");

    // 6. A dashed path.
    let start = c - DASHED_LENGTH / 2.0;
    let dashes: Vec<(f32, f32)> = (0..(DASHED_LENGTH / (2.0 * DASH)) as u32)
      .map(|k| (k as f32 * 2.0 * DASH, k as f32 * 2.0 * DASH + DASH))
      .collect();
    let mut stroke = solid(GREEN);
    stroke.set_draw_style(DrawStyle::Stroke).set_stroke_width(DASH_WIDTH).set_stroke_cap(StrokeCap::Butt);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_path(&line_path(start, &dashes), &stroke);
      }),
    );
    save("dashed", &pixels);
    expect_painted("dashed", &pixels, pixel_of(project(&m, start + DASH / 2.0, c)));
    expect_clear("dashed", &pixels, pixel_of(project(&m, start + DASH * 1.5, c)));
    println!("ok: a dashed path under perspective paints its dash and leaves its gap clear");

    // 7. A gradient stroke along the line.
    let end = start + DASHED_LENGTH;
    let mut graded = gradient_paint(start, end);
    graded.set_draw_style(DrawStyle::Stroke).set_stroke_width(STROKE_WIDTH).set_stroke_cap(StrokeCap::Butt);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.transform(&m).draw_path(&line_path(start, &[(0.0, DASHED_LENGTH)]), &graded);
      }),
    );
    save("stroke", &pixels);
    let at = pixel_of(project(&m, (start + end) / 2.0, c));
    let worst = expect_gradient("stroke", &pixels, &m, at, start, end);
    println!("ok: a gradient stroke under perspective reads its local t (largest channel difference {worst:.2})");

    // 8. No perspective: an image source rotated on a rect.
    let to_draw = Matrix::translation(-edge / 2.0, -edge / 2.0, 0.0)
      .then(&Matrix::scale(IMAGE_SCALE, IMAGE_SCALE, 1.0))
      .then(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(IMAGE_DEGREES)))
      .then(&Matrix::translation(c, c, 0.0));
    let source =
      ColorSource::image(&checker, TileMode::Clamp, TileMode::Clamp, TextureSampling::Nearest, Some(&to_draw));
    let mut image = solid(Color::new_srgba(0.0, 0.0, 0.0, 1.0));
    image.set_color_source(&source);
    let square = Rect::new(Point::zero(), Size::new(SIZE as f32, SIZE as f32));
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_rect(&square, &image);
      }),
    );
    save("image", &pixels);
    let to_texel = to_draw.to_2d().inverse().expect("the image map is invertible");
    for (i, texel) in CHECKER_TEXELS.iter().enumerate() {
      let cell = Point::new((i as u32 % CHECKER) as f32 + 0.5, (i as u32 / CHECKER) as f32 + 0.5);
      let at = to_draw.to_2d().transform_point(cell);
      let at = pixel_of((at.x, at.y));
      let (px, py) = pixel_center(at.0, at.1);
      let back = to_texel.transform_point(Point::new(px, py));
      let index = (back.y.floor() as usize) * CHECKER as usize + back.x.floor() as usize;
      assert_eq!(index, i, "image: pixel {at:?} maps back into cell {index}, want {i}");
      expect_color("image", &pixels, at, *texel);
    }
    println!("ok: an image source rotated {IMAGE_DEGREES} degrees reads the texel its inverse map gives");

    // The batches: a solid rect then a gradient rect two draws, two
    // gradient rects one.
    let half = Rect::new(Point::zero(), Size::new(c, c));
    let mixed = list(|b| {
      b.draw_rect(&half, &solid(GREEN)).draw_rect(&half, &gradient_paint(0.0, c));
    });
    let graded = list(|b| {
      b.draw_rect(&half, &gradient_paint(0.0, c)).draw_rect(&half, &gradient_paint(c, 0.0));
    });
    let mixed_calls = draw_calls(&ctx, || {
      render(&ctx, &mixed);
    });
    let graded_calls = draw_calls(&ctx, || {
      render(&ctx, &graded);
    });
    assert_eq!((mixed_calls, graded_calls), (2, 1), "draw calls: solid then gradient, two gradients");
    println!("ok: solid and gradient rects draw in {mixed_calls} calls, two gradient rects in {graded_calls}");

    println!("all transform cases verified");
    std::process::exit(0);
  });
}
