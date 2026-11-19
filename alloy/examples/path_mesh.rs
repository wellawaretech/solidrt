//! Paths drawn through their tessellated meshes (stage 4, steps 2 and 3 of
//! okf/plans/own-renderer.md), on the real GL path: anvil's mesh program
//! and its gradient and textured variants, the mesh cache, and the
//! counts alloy sums into its raster stats.
//!
//! Verified here, each case a list of its own on a clear target:
//!   1. a filled five-point star: under NonZero the center is painted,
//!      under EvenOdd the center is clear while a tip is painted;
//!   2. two segments meeting at 30 degrees, stroked 12 wide: a miter join
//!      paints the miter's tip, a bevel join leaves it clear, and a round
//!      join paints its arc where the bevel is clear;
//!   3. a segment stroked 16 wide: a round cap paints past the end on the
//!      axis, a butt cap does not, a square cap paints the square's corner
//!      the round cap leaves clear;
//!   4. a cubic dashed by hand into two pieces with a gap: a pixel on a
//!      dash is painted, one in the gap is clear;
//!   5. a 64x64 square path with a linear gradient, against the CPU's
//!      colors at the left edge, the middle and the right edge; the same
//!      square filled with a 16x16 image, its corners the image's corner
//!      texels;
//!   6. a full-target square path, solid, gradient and image filled, under
//!      a rect clip of the left half (the right half clear, its mirror
//!      painted), a rounded clip (the corner clear, the center painted)
//!      and a 45 degree rotation about the center (the target's corner
//!      clear, the center painted);
//!   7. the mesh cache, through `Context::raster_counters`: the first draw
//!      of a star tessellates one mesh, a second draw of the same list
//!      none, and the same path under a scale of 2 one again; every draw
//!      draws one mesh;
//!   8. a stroke-and-fill star: the center is painted, and the stroke
//!      paints past the fill's tip where the fill alone is clear.
//!
//! Run: cargo run -p alloy --example path_mesh

use alloy::renderer::prelude::*;
use alloy::renderer::{
  ClipOperation, Color, ColorSource, DisplayList, DisplayListBuilder, DrawStyle, FillType, ISize, Matrix, Paint, Path,
  PathBuilder, Point, Rect, RoundingRadii, Size, StrokeCap, StrokeJoin, Texture, TextureSampling, TileMode,
};
use alloy::{Context, SamplerState, TextureFormat};
use lyon_path::geom::CubicBezierSegment;

// The target's edge in pixels.
const SIZE: u32 = 128;
// The largest per-channel difference from an expected color: the 8-bit
// ramp and the MSAA resolve of a pixel the edge does not cross.
const TOLERANCE: f32 = 2.0;
// The middle pixel of the target, on both axes.
const MIDDLE: u32 = SIZE / 2;
// A pixel nothing drew.
const CLEAR: [u8; 4] = [0, 0, 0, 0];
// The most a level-valued channel reads: 8-bit.
const LEVELS: f32 = 255.0;
// The probe of a target's corner, and of a shape's edge: one pixel in from it.
const CORNER: u32 = 1;

// Case 1: the star's outer radius, and how far below the top tip its tip
// probe sits (where the tip is wide enough to cover the pixel).
const STAR_RADIUS: f32 = 56.0;
const TIP_PROBE: u32 = 12;
// The star's points, and the step between consecutive vertices of the
// pentagram (every second point), in units of a point's angle.
const STAR_POINTS: u32 = 5;
const STAR_STEP: u32 = 2;

// Case 2: the stroke width, the angle between the two segments, their
// length, and where they meet (the join's vertex). The miter ratio at 30
// degrees, 1 / sin(15 degrees) = 3.86, is under the default limit of 4,
// so the miter is drawn, reaching 23 px past the vertex; the bevel's edge
// is 1.6 px past it, the round join's arc 6 px.
const JOIN_WIDTH: f32 = 12.0;
const JOIN_ANGLE_DEGREES: f32 = 30.0;
const JOIN_LENGTH: f32 = 64.0;
const JOIN_X: f32 = 80.0;
// The probes along the join's outward bisector, in pixels past the
// vertex: one inside the round arc and past the bevel, one inside the
// miter and past the arc.
const ROUND_PROBE: u32 = 3;
const MITER_PROBE: u32 = 12;

// Case 3: the stroke width, the segment's ends, and the probes past the
// end: on the axis (inside the round cap's radius), and diagonally (inside
// the square cap's corner, outside the round cap's radius).
const CAP_WIDTH: f32 = 16.0;
const CAP_START: f32 = 24.0;
const CAP_END: f32 = 80.0;
const CAP_PROBE: u32 = 5;
const CAP_CORNER_PROBE: u32 = 6;

// Case 4: the curve's stroke width, its control points, and the parameter
// ranges of the two dashes; the gap's probe is the curve's apex at t 0.5,
// the dash's probe the point at DASH_PROBE_T.
const DASH_WIDTH: f32 = 8.0;
const CURVE: [[f32; 2]; 4] = [[16.0, 96.0], [40.0, 16.0], [88.0, 16.0], [112.0, 96.0]];
const FIRST_DASH: std::ops::Range<f32> = 0.0..0.4;
const SECOND_DASH: std::ops::Range<f32> = 0.6..1.0;
const DASH_PROBE_T: f32 = 0.2;
const GAP_PROBE_T: f32 = 0.5;

// Case 5: the square's origin and edge, and the image's edge in texels.
const SQUARE_ORIGIN: f32 = 32.0;
const SQUARE_EDGE: f32 = 64.0;
const IMAGE_EDGE: u32 = 16;
// How many levels the image climbs per texel on x and on y.
const IMAGE_STEP: u32 = 16;
// The image's constant blue channel.
const IMAGE_BLUE: u8 = 128;

// Case 6: the rounded clip's radius and the rotation's angle.
const CLIP_RADIUS: f32 = 32.0;
const ROTATION_DEGREES: f32 = 45.0;

// Case 7: the scale that moves a draw to the next scale bucket.
const BUCKET_SCALE: f32 = 2.0;

// Case 8: the stroke-and-fill star's stroke width, and how far above the
// top tip its probe sits: inside the stroke's miter, outside the fill.
const OUTLINE_WIDTH: f32 = 8.0;
const OUTLINE_PROBE: u32 = 3;

const GREEN: Color = Color::new_srgba(0.0, 1.0, 0.0, 1.0);
const RED: Color = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
const BLUE: Color = Color::new_srgba(0.0, 0.0, 1.0, 1.0);

fn render(ctx: &Context, list: &DisplayList) -> Vec<u8> {
  let texture = ctx.render_display_list_to_texture(list, SIZE, SIZE, true).expect("rasterize the list");
  ctx.read_texture(&texture, SIZE, SIZE).expect("read the target back")
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
  let i = ((y * SIZE + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

// Whether two pixels are within the tolerance of each other.
fn close(a: [u8; 4], b: [u8; 4]) -> bool {
  a.iter().zip(b.iter()).all(|(a, b)| (*a as f32 - *b as f32).abs() <= TOLERANCE)
}

fn save(name: &str, pixels: &[u8]) {
  let path = format!("/tmp/path_mesh_{name}.png");
  image::save_buffer(&path, pixels, SIZE, SIZE, image::ColorType::Rgba8).expect("save png");
  println!("saved {path}");
}

fn list(build: impl FnOnce(&mut DisplayListBuilder)) -> DisplayList {
  let mut b = DisplayListBuilder::new(None);
  build(&mut b);
  b.build().expect("build the list")
}

// An opaque color in levels.
fn levels(color: Color) -> [u8; 4] {
  let l = |c: f32| (c * LEVELS).round() as u8;
  [l(color.red), l(color.green), l(color.blue), l(color.alpha)]
}

fn expect_color(case: &str, pixels: &[u8], x: u32, y: u32, want: [u8; 4]) {
  let got = pixel(pixels, x, y);
  assert!(close(got, want), "{case}: pixel ({x}, {y}) is {got:?}, want {want:?}");
}

fn expect_clear(case: &str, pixels: &[u8], x: u32, y: u32) {
  let got = pixel(pixels, x, y);
  assert_eq!(got, CLEAR, "{case}: pixel ({x}, {y}) is {got:?}, want clear");
}

// Painted: opaque, whatever the color (a gradient's or an image's).
fn expect_painted(case: &str, pixels: &[u8], x: u32, y: u32) {
  let got = pixel(pixels, x, y);
  assert!(got[3] as f32 >= LEVELS - TOLERANCE, "{case}: pixel ({x}, {y}) is {got:?}, want painted");
}

fn solid(style: DrawStyle) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(GREEN).set_draw_style(style);
  paint
}

fn stroke(width: f32, cap: StrokeCap, join: StrokeJoin) -> Paint {
  let mut paint = solid(DrawStyle::Stroke);
  paint.set_stroke_width(width).set_stroke_cap(cap).set_stroke_join(join);
  paint
}

fn point(p: [f32; 2]) -> Point {
  Point::new(p[0], p[1])
}

// The pentagram about the target's center, its first point straight up:
// the vertices visited every second point, so the edges cross and the
// center pentagon has a winding number of 2.
fn star(fill: FillType) -> Path {
  let center = MIDDLE as f32;
  let vertex = |k: u32| {
    let angle = (-90.0 + (k * STAR_STEP) as f32 * 360.0 / STAR_POINTS as f32).to_radians();
    Point::new(center + STAR_RADIUS * angle.cos(), center + STAR_RADIUS * angle.sin())
  };
  let mut b = PathBuilder::default();
  b.move_to(vertex(0));
  for k in 1..STAR_POINTS {
    b.line_to(vertex(k));
  }
  b.close();
  b.build(fill)
}

// The y pixel of the star's top tip.
fn star_tip() -> u32 {
  (MIDDLE as f32 - STAR_RADIUS) as u32
}

fn square(origin: f32, edge: f32) -> Path {
  let mut b = PathBuilder::default();
  b.move_to(Point::new(origin, origin))
    .line_to(Point::new(origin + edge, origin))
    .line_to(Point::new(origin + edge, origin + edge))
    .line_to(Point::new(origin, origin + edge));
  b.close();
  b.build(FillType::NonZero)
}

// The image: a red-green ramp over x and y, opaque.
fn image_pixels() -> Vec<u8> {
  let mut pixels = Vec::with_capacity((IMAGE_EDGE * IMAGE_EDGE * 4) as usize);
  for y in 0..IMAGE_EDGE {
    for x in 0..IMAGE_EDGE {
      pixels.extend_from_slice(&[(x * IMAGE_STEP) as u8, (y * IMAGE_STEP) as u8, IMAGE_BLUE, 255]);
    }
  }
  pixels
}

fn texel(x: u32, y: u32) -> [u8; 4] {
  [(x * IMAGE_STEP) as u8, (y * IMAGE_STEP) as u8, IMAGE_BLUE, 255]
}

// A paint filled with `image`, its texels mapped onto the square at
// `origin` with edge `edge`.
fn image_paint(image: &Texture, origin: f32, edge: f32) -> Paint {
  let scale = edge / IMAGE_EDGE as f32;
  let to_draw = Matrix::new_2d(scale, 0.0, 0.0, scale, origin, origin);
  let source = ColorSource::image(image, TileMode::Clamp, TileMode::Clamp, TextureSampling::Nearest, Some(&to_draw));
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, 1.0)).set_color_source(&source);
  paint
}

// A paint with the red-to-blue gradient from x `from` to x `to`, clamped.
fn gradient_paint(from: f32, to: f32) -> Paint {
  let source = ColorSource::linear_gradient(
    Point::new(from, 0.0),
    Point::new(to, 0.0),
    &[RED, BLUE],
    &[0.0, 1.0],
    TileMode::Clamp,
    None,
  );
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, 1.0)).set_color_source(&source);
  paint
}

// The gradient's color at the pixel x `x`'s center, in levels.
fn gradient_at(x: u32, from: f32, to: f32) -> [f32; 4] {
  let t = ((x as f32 + 0.5 - from) / (to - from)).clamp(0.0, 1.0);
  let m = |a: f32, b: f32| (a + (b - a) * t) * LEVELS;
  [m(RED.red, BLUE.red), m(RED.green, BLUE.green), m(RED.blue, BLUE.blue), LEVELS]
}

fn expect_gradient(case: &str, pixels: &[u8], x: u32, y: u32, want: [f32; 4]) -> f32 {
  let got = pixel(pixels, x, y);
  let diff = got.iter().zip(want.iter()).map(|(g, w)| (*g as f32 - w).abs()).fold(0.0, f32::max);
  assert!(diff <= TOLERANCE, "{case}: pixel ({x}, {y}) is {got:?}, want {want:?}");
  diff
}

// The two-segment polyline of case 2: from the upper arm in to the vertex
// and out along the lower arm, the outward bisector pointing along +x on
// the middle row (through its pixels' centers).
fn join_path() -> Path {
  let half = (JOIN_ANGLE_DEGREES / 2.0).to_radians();
  let y = MIDDLE as f32 + 0.5;
  let (dx, dy) = (JOIN_LENGTH * half.cos(), JOIN_LENGTH * half.sin());
  let mut b = PathBuilder::default();
  b.move_to(Point::new(JOIN_X - dx, y - dy)).line_to(Point::new(JOIN_X, y)).line_to(Point::new(JOIN_X - dx, y + dy));
  b.build(FillType::NonZero)
}

fn cap_path() -> Path {
  let y = MIDDLE as f32 + 0.5;
  let mut b = PathBuilder::default();
  b.move_to(Point::new(CAP_START, y)).line_to(Point::new(CAP_END, y));
  b.build(FillType::NonZero)
}

// The raster counters' mesh and tessellation counts after `draw`, as the
// growth over their counts before it.
fn counted(ctx: &Context, draw: impl FnOnce()) -> (u64, u64) {
  let before = ctx.raster_counters();
  draw();
  let after = ctx.raster_counters();
  (after.renderer_meshes - before.renderer_meshes, after.renderer_tessellated - before.renderer_tessellated)
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "path mesh probe", ISize::new(400, 300), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let full = Rect::new(Point::zero(), Size::new(SIZE as f32, SIZE as f32));
    let tip = star_tip();

    // 1. The star under both fill rules.
    let nonzero = render(
      &ctx,
      &list(|b| {
        b.draw_path(&star(FillType::NonZero), &solid(DrawStyle::Fill));
      }),
    );
    save("star_nonzero", &nonzero);
    expect_color("nonzero", &nonzero, MIDDLE, MIDDLE, levels(GREEN));
    expect_color("nonzero", &nonzero, MIDDLE, tip + TIP_PROBE, levels(GREEN));
    let evenodd = render(
      &ctx,
      &list(|b| {
        b.draw_path(&star(FillType::EvenOdd), &solid(DrawStyle::Fill));
      }),
    );
    save("star_evenodd", &evenodd);
    expect_clear("evenodd", &evenodd, MIDDLE, MIDDLE);
    expect_color("evenodd", &evenodd, MIDDLE, tip + TIP_PROBE, levels(GREEN));
    println!("ok: a star fills its center under NonZero and leaves it clear under EvenOdd, tips painted in both");

    // 2. Joins.
    let joined = |join: StrokeJoin| {
      render(
        &ctx,
        &list(|b| {
          b.draw_path(&join_path(), &stroke(JOIN_WIDTH, StrokeCap::Butt, join));
        }),
      )
    };
    let (round_x, miter_x) = (JOIN_X as u32 + ROUND_PROBE, JOIN_X as u32 + MITER_PROBE);
    let miter = joined(StrokeJoin::Miter);
    save("join_miter", &miter);
    expect_color("miter", &miter, miter_x, MIDDLE, levels(GREEN));
    expect_color("miter", &miter, round_x, MIDDLE, levels(GREEN));
    let bevel = joined(StrokeJoin::Bevel);
    save("join_bevel", &bevel);
    expect_clear("bevel", &bevel, miter_x, MIDDLE);
    expect_clear("bevel", &bevel, round_x, MIDDLE);
    let round = joined(StrokeJoin::Round);
    save("join_round", &round);
    expect_color("round", &round, round_x, MIDDLE, levels(GREEN));
    expect_clear("round", &round, miter_x, MIDDLE);
    println!("ok: a miter join paints its tip, a bevel leaves it clear, a round join paints its arc");

    // 3. Caps.
    let capped = |cap: StrokeCap| {
      render(
        &ctx,
        &list(|b| {
          b.draw_path(&cap_path(), &stroke(CAP_WIDTH, cap, StrokeJoin::Miter));
        }),
      )
    };
    let (axis_x, corner_x, corner_y) =
      (CAP_END as u32 + CAP_PROBE, CAP_END as u32 + CAP_CORNER_PROBE, MIDDLE + CAP_CORNER_PROBE);
    let round = capped(StrokeCap::Round);
    save("cap_round", &round);
    expect_color("round cap", &round, axis_x, MIDDLE, levels(GREEN));
    expect_clear("round cap", &round, corner_x, corner_y);
    let butt = capped(StrokeCap::Butt);
    save("cap_butt", &butt);
    expect_clear("butt cap", &butt, axis_x, MIDDLE);
    let square_cap = capped(StrokeCap::Square);
    save("cap_square", &square_cap);
    expect_color("square cap", &square_cap, corner_x, corner_y, levels(GREEN));
    println!("ok: a round cap paints past the end, a butt cap does not, a square cap paints its corner");

    // 4. A dashed curve: the two dashes as pieces of the curve, the gap
    // between them undrawn.
    let curve =
      CubicBezierSegment { from: point(CURVE[0]), ctrl1: point(CURVE[1]), ctrl2: point(CURVE[2]), to: point(CURVE[3]) };
    let mut b = PathBuilder::default();
    for dash in [FIRST_DASH, SECOND_DASH] {
      let piece = curve.split_range(dash);
      b.move_to(piece.from).cubic_to(piece.ctrl1, piece.ctrl2, piece.to);
    }
    let dashed = b.build(FillType::NonZero);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_path(&dashed, &stroke(DASH_WIDTH, StrokeCap::Butt, StrokeJoin::Miter));
      }),
    );
    save("dashed", &pixels);
    let (on, gap) = (curve.sample(DASH_PROBE_T), curve.sample(GAP_PROBE_T));
    expect_color("dash", &pixels, on.x as u32, on.y as u32, levels(GREEN));
    expect_clear("gap", &pixels, gap.x as u32, gap.y as u32);
    println!("ok: a dashed curve paints its dashes and leaves its gap clear");

    // 5. A gradient and an image filled square.
    let (left, right) = (SQUARE_ORIGIN, SQUARE_ORIGIN + SQUARE_EDGE);
    let (first, last) = (left as u32 + CORNER, right as u32 - 1 - CORNER);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_path(&square(SQUARE_ORIGIN, SQUARE_EDGE), &gradient_paint(left, right));
      }),
    );
    save("gradient", &pixels);
    let worst = [first, MIDDLE, last]
      .iter()
      .map(|x| expect_gradient("gradient", &pixels, *x, MIDDLE, gradient_at(*x, left, right)))
      .fold(0.0, f32::max);
    expect_clear("gradient", &pixels, CORNER, CORNER);
    println!("ok: a gradient path runs red to blue across its square (largest channel difference {worst:.2})");

    let image_id = ctx
      .create_texture_from_pixels(
        IMAGE_EDGE,
        IMAGE_EDGE,
        &image_pixels(),
        SamplerState::default(),
        TextureFormat::Rgba8,
        Some("path image".into()),
      )
      .expect("create the image");
    let image: Texture =
      ctx.textures.get(image_id).and_then(|e| e.handle.clone()).expect("the image has a renderer handle");
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_path(&square(SQUARE_ORIGIN, SQUARE_EDGE), &image_paint(&image, SQUARE_ORIGIN, SQUARE_EDGE));
      }),
    );
    save("image", &pixels);
    let end = IMAGE_EDGE - 1;
    for (x, y, tx, ty) in [(first, first, 0, 0), (last, first, end, 0), (first, last, 0, end), (last, last, end, end)] {
      expect_color("image", &pixels, x, y, texel(tx, ty));
    }
    println!("ok: an image filled path samples the image's corner texels at its corners");

    // 6. A full-target square under a rect clip, a rounded clip and a
    // rotation, for each fill.
    let edge = SIZE as f32;
    let fills = [
      ("solid", solid(DrawStyle::Fill)),
      ("gradient", gradient_paint(0.0, edge)),
      ("image", image_paint(&image, 0.0, edge)),
    ];
    let (sin, cos) = ROTATION_DEGREES.to_radians().sin_cos();
    let half = edge / 2.0;
    for (name, paint) in &fills {
      let target = square(0.0, edge);
      let clipped = render(
        &ctx,
        &list(|b| {
          b.clip_rect(&Rect::new(Point::zero(), Size::new(half, edge)), ClipOperation::Intersect);
          b.draw_path(&target, paint);
        }),
      );
      save(&format!("clip_rect_{name}"), &clipped);
      expect_clear(name, &clipped, SIZE - 1 - CORNER, MIDDLE);
      expect_painted(name, &clipped, CORNER, MIDDLE);

      let rounded = render(
        &ctx,
        &list(|b| {
          b.clip_rounded_rect(&full, &RoundingRadii::circular([CLIP_RADIUS; 4]), ClipOperation::Intersect);
          b.draw_path(&target, paint);
        }),
      );
      save(&format!("clip_rounded_{name}"), &rounded);
      expect_clear(name, &rounded, CORNER, CORNER);
      expect_painted(name, &rounded, MIDDLE, MIDDLE);

      let rotated = render(
        &ctx,
        &list(|b| {
          b.translate(half, half);
          b.transform(&Matrix::new_2d(cos, sin, -sin, cos, 0.0, 0.0));
          b.translate(-half, -half);
          b.draw_path(&target, paint);
        }),
      );
      save(&format!("rotated_{name}"), &rotated);
      expect_clear(name, &rotated, CORNER, CORNER);
      expect_painted(name, &rotated, MIDDLE, MIDDLE);
    }
    println!("ok: solid, gradient and image paths keep a rect clip, a rounded clip and a rotation");

    // 7. The mesh cache: a fresh path, drawn twice, then at the next scale
    // bucket.
    let cached = star(FillType::NonZero);
    let once = list(|b| {
      b.draw_path(&cached, &solid(DrawStyle::Fill));
    });
    let first_draw = counted(&ctx, || {
      render(&ctx, &once);
    });
    let second_draw = counted(&ctx, || {
      render(&ctx, &once);
    });
    let doubled = list(|b| {
      b.scale(BUCKET_SCALE, BUCKET_SCALE);
      b.draw_path(&cached, &solid(DrawStyle::Fill));
    });
    let scaled_draw = counted(&ctx, || {
      render(&ctx, &doubled);
    });
    assert_eq!(first_draw, (1, 1), "the first draw tessellates the path's one mesh (meshes, tessellated)");
    assert_eq!(second_draw, (1, 0), "a second draw of the same list redraws from the cache (meshes, tessellated)");
    assert_eq!(scaled_draw, (1, 1), "a doubled scale is the next bucket's mesh (meshes, tessellated)");
    println!(
      "ok: (meshes, tessellated) per draw: first {first_draw:?}, again {second_draw:?}, doubled {scaled_draw:?}"
    );

    // 8. Stroke and fill.
    let mut outline = solid(DrawStyle::StrokeAndFill);
    outline.set_stroke_width(OUTLINE_WIDTH);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_path(&star(FillType::NonZero), &outline);
      }),
    );
    save("stroke_and_fill", &pixels);
    let above = tip - OUTLINE_PROBE;
    expect_color("stroke and fill", &pixels, MIDDLE, MIDDLE, levels(GREEN));
    expect_color("stroke and fill", &pixels, MIDDLE, above, levels(GREEN));
    expect_clear("fill alone", &nonzero, MIDDLE, above);
    println!("ok: a stroke-and-fill star fills its center and strokes past the fill's tip");

    println!("all path mesh cases verified");
    std::process::exit(0);
  });
}
