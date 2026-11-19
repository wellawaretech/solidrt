//! Gradients on every quad (stage 4, step 1 of okf/plans/own-renderer.md),
//! on the real GL path: a paint's linear or radial gradient source drawn
//! by the fill and the shape program's gradient variants, evaluated per
//! device pixel from the rows the walk composed.
//!
//! Verified here, each against the color the gradient has at the pixel's
//! center, within two levels per channel (the ramp is 8-bit and sampled
//! bilinearly):
//!   1. a full-target rect with a linear gradient, red to blue across x,
//!      clamped: the left edge, the right edge and the middle;
//!   2. the same gradient on a half-size rect under a scale of 2, the
//!      gradient's axis in the op's space: the same three pixels (the rows
//!      composed with the device map);
//!   3. a radial gradient, white to transparent, decal: the center, a
//!      corner outside the radius (clear) and a point halfway out;
//!   4. a repeating linear gradient: a pixel and the pixel one period on
//!      match;
//!   5. a rounded rect under a rounded clip of the same rect: the corner
//!      is clear, the center is the middle color;
//!   6. the gradient rect rotated 45 degrees about the center: the color
//!      changes along the rotated axis and holds across it;
//!   7. a gradient line across the target, butt caps: the three pixels of
//!      case 1;
//!   8. a gradient rect at half paint alpha: the center pixel is the
//!      middle color premultiplied, about half alpha.
//!
//! Run: cargo run -p alloy --example gradient_fill

use alloy::renderer::prelude::*;
use alloy::renderer::{
  ClipOperation, Color, ColorSource, DisplayList, DisplayListBuilder, DrawStyle, ISize, Matrix, Paint, Point, Rect,
  RoundingRadii, Size, StrokeCap, TileMode,
};
use alloy::Context;

// The target's edge in pixels.
const SIZE: u32 = 128;
// The largest per-channel difference from the CPU-evaluated color: the
// ramp's 8-bit texels and its bilinear lookup.
const TOLERANCE: f32 = 2.0;
// The probe pixels of the linear cases: one in from each edge, and the
// middle, on the middle row.
const LEFT: u32 = 1;
const RIGHT: u32 = SIZE - 2;
const MIDDLE: u32 = SIZE / 2;
// Case 2's map: the rect and the gradient axis are half the target, drawn
// at this scale.
const SCALE: f32 = 2.0;
// Case 3's radius, and how far below the center its halfway probe sits.
const RADIAL_RADIUS: f32 = 48.0;
const RADIAL_PROBE: u32 = 24;
// The corner probe outside the radial gradient.
const CORNER: u32 = 2;
// Case 4's period, and the probe's x within the first one.
const PERIOD: f32 = 32.0;
const REPEAT_PROBE: u32 = 8;
// Case 5's corner radius.
const RADIUS: f32 = 32.0;
// Case 6's angle, and how far from the center its probes sit per axis.
const ROTATION_DEGREES: f32 = 45.0;
const ROTATED_PROBE: u32 = 20;
// Case 7's stroke width.
const LINE_WIDTH: f32 = 16.0;
// Where the line case's segment starts, in pixels, and the probe just past
// it: the left probe lies before the segment and must read clear.
const LINE_START: f32 = 32.0;
const LINE_PROBE: u32 = LINE_START as u32 + 2;
// Case 8's paint alpha.
const HALF_ALPHA: f32 = 0.5;
// The most a level-valued channel reads: 8-bit.
const LEVELS: f32 = 255.0;
// A pixel nothing drew.
const CLEAR: [u8; 4] = [0, 0, 0, 0];

const RED: Color = Color::new_srgba(1.0, 0.0, 0.0, 1.0);
const BLUE: Color = Color::new_srgba(0.0, 0.0, 1.0, 1.0);
const WHITE: Color = Color::new_srgba(1.0, 1.0, 1.0, 1.0);
const CLEAR_WHITE: Color = Color::new_srgba(1.0, 1.0, 1.0, 0.0);

fn full() -> Rect {
  Rect::new(Point::zero(), Size::new(SIZE as f32, SIZE as f32))
}

fn render(ctx: &Context, list: &DisplayList) -> Vec<u8> {
  let texture = ctx.render_display_list_to_texture(list, SIZE, SIZE, true).expect("rasterize the list");
  ctx.read_texture(&texture, SIZE, SIZE).expect("read the target back")
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
  let i = ((y * SIZE + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

// Whether two read pixels are within the tolerance of each other.
fn close(a: [u8; 4], b: [u8; 4]) -> bool {
  a.iter().zip(b.iter()).all(|(a, b)| (*a as f32 - *b as f32).abs() <= TOLERANCE)
}

fn save(name: &str, pixels: &[u8]) {
  let path = format!("/tmp/gradient_fill_{name}.png");
  image::save_buffer(&path, pixels, SIZE, SIZE, image::ColorType::Rgba8).expect("save png");
  println!("saved {path}");
}

// `color` at `alpha` (the paint's), premultiplied, in levels: what the
// target holds after one source-over draw onto clear.
fn levels(color: Color, alpha: f32) -> [f32; 4] {
  let a = color.alpha * alpha;
  [color.red * a * LEVELS, color.green * a * LEVELS, color.blue * a * LEVELS, a * LEVELS]
}

// Two stops' straight-alpha mix at `t`, clamped to 0..1.
fn mix(a: Color, b: Color, t: f32) -> Color {
  let t = t.clamp(0.0, 1.0);
  let m = |x: f32, y: f32| x + (y - x) * t;
  Color::new_srgba(m(a.red, b.red), m(a.green, b.green), m(a.blue, b.blue), m(a.alpha, b.alpha))
}

// Assert the pixel (x, y) of `pixels` is `want` (levels) within the
// tolerance; returns the largest channel difference.
fn expect(case: &str, pixels: &[u8], x: u32, y: u32, want: [f32; 4]) -> f32 {
  let got = pixel(pixels, x, y);
  let diff = got.iter().zip(want.iter()).map(|(g, w)| (*g as f32 - w).abs()).fold(0.0, f32::max);
  assert!(diff <= TOLERANCE, "{case}: pixel ({x}, {y}) is {got:?}, want {want:?}");
  diff
}

// The linear red-to-blue gradient from the origin to (`length`, 0), tiled
// `tile`, as a paint at `alpha`.
fn linear(length: f32, tile: TileMode, alpha: f32) -> Paint {
  let source =
    ColorSource::linear_gradient(Point::zero(), Point::new(length, 0.0), &[RED, BLUE], &[0.0, 1.0], tile, None);
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, alpha)).set_color_source(&source);
  paint
}

// The color of the red-to-blue gradient from 0 to `length` at the device x
// `x`'s pixel center, at `alpha`.
fn linear_at(x: u32, length: f32, alpha: f32) -> [f32; 4] {
  levels(mix(RED, BLUE, (x as f32 + 0.5) / length), alpha)
}

fn list(build: impl FnOnce(&mut DisplayListBuilder)) -> DisplayList {
  let mut b = DisplayListBuilder::new(None);
  build(&mut b);
  b.build().expect("build the list")
}

// The three probes of a full-width red-to-blue gradient on the middle row.
fn three_probes(case: &str, pixels: &[u8]) -> f32 {
  let length = SIZE as f32;
  [LEFT, RIGHT, MIDDLE].iter().map(|x| expect(case, pixels, *x, MIDDLE, linear_at(*x, length, 1.0))).fold(0.0, f32::max)
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "gradient fill probe", ISize::new(400, 300), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let opaque = linear(SIZE as f32, TileMode::Clamp, 1.0);
    let half = SIZE as f32 / 2.0;

    // 1. A full-target rect.
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_rect(&full(), &opaque);
      }),
    );
    save("rect", &pixels);
    let worst = three_probes("rect", &pixels);
    println!("ok: a linear gradient rect runs red to blue (largest channel difference {worst:.2})");

    // 2. The same through a scale: the rect and the axis in the op's space.
    let scaled = linear(half, TileMode::Clamp, 1.0);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.translate(0.0, 0.0).scale(SCALE, SCALE);
        b.draw_rect(&Rect::new(Point::zero(), Size::new(half, half)), &scaled);
      }),
    );
    save("scaled", &pixels);
    let worst = three_probes("scaled", &pixels);
    println!("ok: under a scale the rows compose with the device map (largest channel difference {worst:.2})");

    // 3. A radial gradient, decal.
    let center = Point::new(half, half);
    let source =
      ColorSource::radial_gradient(center, RADIAL_RADIUS, &[WHITE, CLEAR_WHITE], &[0.0, 1.0], TileMode::Decal, None);
    let mut radial = Paint::default();
    radial.set_color_source(&source);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_rect(&full(), &radial);
      }),
    );
    save("radial", &pixels);
    let radial_at = |x: u32, y: u32| {
      let t = (x as f32 + 0.5 - center.x).hypot(y as f32 + 0.5 - center.y) / RADIAL_RADIUS;
      levels(mix(WHITE, CLEAR_WHITE, t), 1.0)
    };
    let below = MIDDLE + RADIAL_PROBE;
    let mut worst = expect("radial", &pixels, MIDDLE, MIDDLE, radial_at(MIDDLE, MIDDLE));
    worst = worst.max(expect("radial", &pixels, MIDDLE, below, radial_at(MIDDLE, below)));
    assert_eq!(pixel(&pixels, CORNER, CORNER), CLEAR, "radial: the corner is past the radius, decal");
    let halfway = pixel(&pixels, MIDDLE, below);
    println!("ok: a radial decal gradient is opaque at the center, clear outside, {halfway:?} halfway out");
    println!("    (largest channel difference {worst:.2})");

    // 4. Repeat.
    let repeat = linear(PERIOD, TileMode::Repeat, 1.0);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_rect(&full(), &repeat);
      }),
    );
    save("repeat", &pixels);
    let next = REPEAT_PROBE + PERIOD as u32;
    let (a, b) = (pixel(&pixels, REPEAT_PROBE, MIDDLE), pixel(&pixels, next, MIDDLE));
    assert!(close(a, b), "repeat: x {REPEAT_PROBE} is {a:?}, x {next} is {b:?}");
    let worst = expect("repeat", &pixels, REPEAT_PROBE, MIDDLE, linear_at(REPEAT_PROBE, PERIOD, 1.0));
    println!("ok: a repeating gradient repeats every period (largest channel difference {worst:.2})");

    // 5. A rounded rect under a rounded clip of the same rect.
    let radii = RoundingRadii::circular([RADIUS; 4]);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.clip_rounded_rect(&full(), &radii, ClipOperation::Intersect);
        b.draw_rounded_rect(&full(), &radii, &opaque);
      }),
    );
    save("rounded", &pixels);
    assert_eq!(pixel(&pixels, LEFT, LEFT), CLEAR, "rounded: the corner is outside the rounding");
    let worst = expect("rounded", &pixels, MIDDLE, MIDDLE, linear_at(MIDDLE, SIZE as f32, 1.0));
    println!("ok: a rounded gradient rect is clear at the corner, mid color at the center (difference {worst:.2})");

    // 6. Rotated 45 degrees about the center.
    let (sin, cos) = ROTATION_DEGREES.to_radians().sin_cos();
    let pixels = render(
      &ctx,
      &list(|b| {
        b.translate(half, half);
        b.transform(&Matrix::new_2d(cos, sin, -sin, cos, 0.0, 0.0));
        b.translate(-half, -half);
        b.draw_rect(&full(), &opaque);
      }),
    );
    save("rotated", &pixels);
    // The local x axis runs down-right in device space.
    let (near, far) = (MIDDLE - ROTATED_PROBE, MIDDLE + ROTATED_PROBE);
    let (start, end) = (pixel(&pixels, near, near), pixel(&pixels, far, far));
    assert!(!close(start, end), "rotated: along the axis the color changes, {start:?} to {end:?}");
    let (up, down) = (pixel(&pixels, far, near), pixel(&pixels, near, far));
    assert!(close(up, down), "rotated: across the axis the color holds, {up:?} and {down:?}");
    // The device point's local x through the rotation, for the CPU color.
    let rotated_at = |x: u32, y: u32| {
      let (dx, dy) = (x as f32 + 0.5 - half, y as f32 + 0.5 - half);
      levels(mix(RED, BLUE, (half + dx * cos + dy * sin) / SIZE as f32), 1.0)
    };
    let mut worst = 0.0f32;
    for (x, y) in [(near, near), (far, far), (far, near), (near, far)] {
      worst = worst.max(expect("rotated", &pixels, x, y, rotated_at(x, y)));
    }
    println!("ok: a rotated gradient changes along its axis and holds across it (difference {worst:.2})");

    // 7. A line across the target, starting inside it: the gradient is the
    // op's, so the probes read the same colors as the rect's although the
    // line's own frame starts LINE_START in (a frame-space evaluation would
    // read them LINE_START / SIZE low).
    let mut stroke = linear(SIZE as f32, TileMode::Clamp, 1.0);
    stroke.set_draw_style(DrawStyle::Stroke).set_stroke_width(LINE_WIDTH).set_stroke_cap(StrokeCap::Butt);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_line(Point::new(LINE_START, half), Point::new(SIZE as f32, half), &stroke);
      }),
    );
    save("line", &pixels);
    let length = SIZE as f32;
    let worst = [LINE_PROBE, MIDDLE, RIGHT]
      .iter()
      .map(|x| expect("line", &pixels, *x, MIDDLE, linear_at(*x, length, 1.0)))
      .fold(0.0, f32::max);
    assert_eq!(pixel(&pixels, LEFT, MIDDLE), CLEAR, "line: nothing is drawn before the segment starts");
    println!("ok: a gradient line runs red to blue (largest channel difference {worst:.2})");

    // 8. Half paint alpha.
    let faded = linear(SIZE as f32, TileMode::Clamp, HALF_ALPHA);
    let pixels = render(
      &ctx,
      &list(|b| {
        b.draw_rect(&full(), &faded);
      }),
    );
    save("faded", &pixels);
    let worst = expect("faded", &pixels, MIDDLE, MIDDLE, linear_at(MIDDLE, SIZE as f32, HALF_ALPHA));
    let center_pixel = pixel(&pixels, MIDDLE, MIDDLE);
    println!("ok: a gradient at half alpha is premultiplied, {center_pixel:?} at the center (difference {worst:.2})");

    println!("all gradient fill cases verified");
    std::process::exit(0);
  });
}
