//! Clips through the stencil (stage 5, step 2 of okf/plans/own-renderer.md),
//! on the real GL path: every case a list of its own, drawn by
//! `render_display_list_to_texture` with antialiasing into the offscreen
//! rig's multisampled target, whose DEPTH24_STENCIL8 attachment holds the
//! stencil per sample (gl/rig.rs `ensure_msaa`), then resolved and read
//! back. Verified:
//!   1. a rect clip rotated about the target's center: a full-target fill
//!      shows at the center, is cut at a corner of the clip's bounding box
//!      outside the rotated square, and outside the box;
//!   2. a rounded clip under the same rotation: a pixel just inside a
//!      corner's arc is kept, one just outside it (still inside the rotated
//!      square) is cut;
//!   3. an oval clip: kept at its center and just inside its rim on an
//!      axis, cut in the corner of its bounds;
//!   4. a difference rect, rounded rect and oval under a translucent fill
//!      covering the target: inside the cutout the target's clear shows,
//!      outside it the fill; the rounded cutout's corner outside the arc is
//!      filled;
//!   5. nested rounded clips, the outer per instance and the inner through
//!      the stencil: kept inside both, cut in the inner's corner outside its
//!      arc and inside the outer only;
//!   6. a rect clip under the card flip with perspective: kept at the
//!      projected center, cut past the projected edge and in the projected
//!      trapezoid's bounding box outside it;
//!   7. an axis-aligned rounded clip above a perspective draw: the
//!      per-instance fast path (no stencil draw), kept at the center, cut
//!      in the clip's corner outside its arc.
//!
//! Every case also reads the stencil clip draws it made from
//! `Context::raster_counters` (`renderer_clips`, the renderer's
//! `DrawReport::clips`).
//!
//! Run: cargo run -p alloy --example clip_cases

use alloy::renderer::prelude::*;
use alloy::renderer::{
  ClipOperation, Color, DisplayList, DisplayListBuilder, ISize, Matrix, Paint, Point, Rect, RoundingRadii, Size,
};
use alloy::Context;

// The target's edge in pixels.
const SIZE: u32 = 256;
// The largest per-channel difference from an expected color: blending
// rounding at 8 bits.
const TOLERANCE: f32 = 2.0;
// The rotation of cases 1 and 2, degrees about the target's center.
const ROTATE_DEGREES: f32 = 30.0;
// The square cases 1 and 2 clip to, centered: its side.
const SQUARE: f32 = 120.0;
// Case 2: the clip's corner radius, and how far a probe sits from the arc,
// in and out along the corner's diagonal: past the MSAA ramp and the
// tessellation's tolerance.
const RADIUS: f32 = 30.0;
const ARC_PROBE: f32 = 3.0;
// Case 1: how far in from the bounding box's corner its probe sits along
// both axes: inside the box, outside the rotated square.
const BOX_CORNER_PROBE: f32 = 6.0;
// Case 3: the oval's radii, and how far in from its rim the axis probe
// sits.
const OVAL_RX: f32 = 90.0;
const OVAL_RY: f32 = 60.0;
const RIM_PROBE: f32 = 3.0;
// Case 4: the cutouts' half side and the rounded cutout's corner radius.
const CUTOUT: f32 = 50.0;
const CUTOUT_RADIUS: f32 = 30.0;
// Case 4: the fill's alpha.
const TRANSLUCENT: f32 = 0.5;
// Case 5: the outer clip and the inner one, x, y, w, h, and their radii.
const OUTER: [f32; 4] = [40.0, 40.0, 176.0, 176.0];
const OUTER_RADIUS: f32 = 20.0;
const INNER: [f32; 4] = [80.0, 60.0, 100.0, 120.0];
const INNER_RADIUS: f32 = 40.0;
// Cases 6 and 7: the card flip's angle and the perspective distance, in
// pixels, and the card it flips.
const FLIP_DEGREES: f32 = 60.0;
const PERSPECTIVE: f32 = 800.0;
const CARD_W: f32 = 200.0;
const CARD_H: f32 = 120.0;
// Case 6: how far the fill reaches past the card, local pixels, and how
// far past the projected edge the outside probe sits.
const CARD_OVERDRAW: f32 = 40.0;
const OUTSIDE_PROBE: f32 = 4.0;
// Case 7: the device rounded clip, x, y, w, h, and its radius.
const DEVICE_CLIP: [f32; 4] = [98.0, 88.0, 60.0, 80.0];
const DEVICE_RADIUS: f32 = 20.0;
// The most a level-valued channel reads: 8-bit.
const LEVELS: f32 = 255.0;
// A pixel nothing drew.
const CLEAR: [u8; 4] = [0, 0, 0, 0];

const GREEN: Color = Color::new_srgba(0.0, 1.0, 0.0, 1.0);

fn center() -> f32 {
  SIZE as f32 / 2.0
}

fn rect(r: [f32; 4]) -> Rect {
  Rect::new(Point::new(r[0], r[1]), Size::new(r[2], r[3]))
}

// The square of `side` centered on the target.
fn centered(side: f32) -> Rect {
  let c = center();
  rect([c - side / 2.0, c - side / 2.0, side, side])
}

fn target() -> Rect {
  rect([0.0, 0.0, SIZE as f32, SIZE as f32])
}

// The rotation by ROTATE_DEGREES about the target's center.
fn rotation() -> Matrix {
  let c = center();
  Matrix::translation(-c, -c, 0.0)
    .then(&Matrix::rotation(0.0, 0.0, 1.0, euclid::Angle::degrees(ROTATE_DEGREES)))
    .then(&Matrix::translation(c, c, 0.0))
}

// The card flip about the target's center, in View::compose_user's order
// (alloy/src/rendertree/kinds/view.rs), as transform_cases composes it.
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

fn card() -> Rect {
  let c = center();
  rect([c - CARD_W / 2.0, c - CARD_H / 2.0, CARD_W, CARD_H])
}

// The device point `m` puts the local point (x, y) on: z = 0, divided by w.
fn project(m: &Matrix, x: f32, y: f32) -> (f32, f32) {
  let p = m.transform_point3d(euclid::default::Point3D::new(x, y, 0.0)).expect("the point is in front of the eye");
  (p.x, p.y)
}

// The pixel the device point (x, y) lies in.
fn pixel_of(p: (f32, f32)) -> (u32, u32) {
  (p.0.floor() as u32, p.1.floor() as u32)
}

fn render(ctx: &Context, list: &DisplayList) -> Vec<u8> {
  let texture = ctx.render_display_list_to_texture(list, SIZE, SIZE, true).expect("rasterize the list");
  ctx.read_texture(&texture, SIZE, SIZE).expect("read the target back")
}

// `list` rendered, and the stencil clip draws its draw made.
fn render_counted(ctx: &Context, list: &DisplayList) -> (Vec<u8>, u64) {
  let before = ctx.raster_counters().renderer_clips;
  let pixels = render(ctx, list);
  (pixels, ctx.raster_counters().renderer_clips - before)
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
  let i = ((y * SIZE + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn save(name: &str, pixels: &[u8]) {
  let path = format!("/tmp/clip_cases_{name}.png");
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

// `color` premultiplied, in levels.
fn levels(color: Color) -> [u8; 4] {
  let l = |v: f32| (v * color.alpha * LEVELS).round() as u8;
  [l(color.red), l(color.green), l(color.blue), (color.alpha * LEVELS).round() as u8]
}

// A case's draw and its restore: `rect` filled with `color` under the clip
// in effect, then the save popped.
fn draw_and_restore(b: &mut DisplayListBuilder, rect: &Rect, color: Color) {
  b.draw_rect(rect, &solid(color)).restore();
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

fn expect_clips(case: &str, got: u64, want: u64) {
  assert_eq!(got, want, "{case}: {got} stencil clip draws, want {want}");
}

// The local points along a rounded corner's diagonal just inside and just
// outside its arc, for the top-left corner of `r` with `radius`.
fn arc_probes(r: &Rect, radius: f32) -> ((f32, f32), (f32, f32)) {
  let (cx, cy) = (r.min_x() + radius, r.min_y() + radius);
  let along = |d: f32| (cx - d * std::f32::consts::FRAC_1_SQRT_2, cy - d * std::f32::consts::FRAC_1_SQRT_2);
  (along(radius - ARC_PROBE), along(radius + ARC_PROBE))
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "clip cases probe", ISize::new(400, 300), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let c = center();
    let green = levels(GREEN);
    let r = rotation();
    let square = centered(SQUARE);

    // 1. A rotated rect clip.
    let (pixels, clips) = render_counted(
      &ctx,
      &list(|b| {
        b.save().transform(&r).clip_rect(&square, ClipOperation::Intersect);
        draw_and_restore(b, &target(), GREEN);
      }),
    );
    save("rotated_rect", &pixels);
    let corners = [
      project(&r, square.min_x(), square.min_y()),
      project(&r, square.max_x(), square.min_y()),
      project(&r, square.min_x(), square.max_y()),
      project(&r, square.max_x(), square.max_y()),
    ];
    let min_x = corners.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let min_y = corners.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    expect_color("rotated rect", &pixels, pixel_of((c, c)), green);
    expect_clear("rotated rect", &pixels, pixel_of((min_x + BOX_CORNER_PROBE, min_y + BOX_CORNER_PROBE)));
    expect_clear("rotated rect", &pixels, (1, 1));
    expect_clips("rotated rect", clips, 2);
    println!("ok: a rotated rect clip keeps its square and cuts its bounding box's corner ({clips} clip draws)");

    // 2. A rotated rounded clip.
    let radii = RoundingRadii::circular([RADIUS; 4]);
    let (pixels, clips) = render_counted(
      &ctx,
      &list(|b| {
        b.save().transform(&r).clip_rounded_rect(&square, &radii, ClipOperation::Intersect);
        draw_and_restore(b, &target(), GREEN);
      }),
    );
    save("rotated_rounded", &pixels);
    let (inside, outside) = arc_probes(&square, RADIUS);
    expect_color("rotated rounded", &pixels, pixel_of(project(&r, inside.0, inside.1)), green);
    expect_clear("rotated rounded", &pixels, pixel_of(project(&r, outside.0, outside.1)));
    expect_color("rotated rounded", &pixels, pixel_of((c, c)), green);
    expect_clips("rotated rounded", clips, 2);
    println!("ok: a rotated rounded clip keeps a pixel just inside its arc and cuts one just outside");

    // 3. An oval clip.
    let oval = rect([c - OVAL_RX, c - OVAL_RY, 2.0 * OVAL_RX, 2.0 * OVAL_RY]);
    let (pixels, clips) = render_counted(
      &ctx,
      &list(|b| {
        b.save().clip_oval(&oval, ClipOperation::Intersect);
        draw_and_restore(b, &target(), GREEN);
      }),
    );
    save("oval", &pixels);
    expect_color("oval", &pixels, pixel_of((c, c)), green);
    expect_color("oval", &pixels, pixel_of((c + OVAL_RX - RIM_PROBE, c)), green);
    expect_clear("oval", &pixels, pixel_of((oval.min_x() + RIM_PROBE, oval.min_y() + RIM_PROBE)));
    expect_clear("oval", &pixels, pixel_of((c + OVAL_RX + RIM_PROBE, c)));
    expect_clips("oval", clips, 2);
    println!("ok: an oval clip keeps its inside and cuts its bounds' corner");

    // 4. Difference clips under a translucent fill.
    let faded = Color::new_srgba(0.0, 1.0, 0.0, TRANSLUCENT);
    let fill = levels(faded);
    let cutout = centered(2.0 * CUTOUT);
    let corner = (cutout.min_x() + 1.0, cutout.min_y() + 1.0);
    let cutout_radii = RoundingRadii::circular([CUTOUT_RADIUS; 4]);
    for (name, clip) in [("difference rect", 0), ("difference rounded", 1), ("difference oval", 2)] {
      let (pixels, clips) = render_counted(
        &ctx,
        &list(|b| {
          b.save();
          match clip {
            0 => b.clip_rect(&cutout, ClipOperation::Difference),
            1 => b.clip_rounded_rect(&cutout, &cutout_radii, ClipOperation::Difference),
            _ => b.clip_oval(&cutout, ClipOperation::Difference),
          };
          draw_and_restore(b, &target(), faded);
        }),
      );
      save(&name.replace(' ', "_"), &pixels);
      expect_clear(name, &pixels, pixel_of((c, c)));
      expect_color(name, &pixels, (1, 1), fill);
      expect_color(name, &pixels, pixel_of((cutout.max_x() + RIM_PROBE, c)), fill);
      if clip == 0 {
        expect_clear(name, &pixels, pixel_of(corner));
      } else {
        // The corner outside the arc is outside the cutout: filled.
        expect_color(name, &pixels, pixel_of(corner), fill);
      }
      if clip == 1 {
        let (inside, outside) = arc_probes(&cutout, CUTOUT_RADIUS);
        expect_clear(name, &pixels, pixel_of(inside));
        expect_color(name, &pixels, pixel_of(outside), fill);
      }
      expect_clips(name, clips, 3);
      println!("ok: a {name} clip shows the clear inside its cutout and the fill outside ({clips} clip draws)");
    }

    // 5. Nested rounded clips.
    let (outer, inner) = (rect(OUTER), rect(INNER));
    let (pixels, clips) = render_counted(
      &ctx,
      &list(|b| {
        b.save()
          .clip_rounded_rect(&outer, &RoundingRadii::circular([OUTER_RADIUS; 4]), ClipOperation::Intersect)
          .clip_rounded_rect(&inner, &RoundingRadii::circular([INNER_RADIUS; 4]), ClipOperation::Intersect);
        draw_and_restore(b, &target(), GREEN);
      }),
    );
    save("nested", &pixels);
    let (inside, outside) = arc_probes(&inner, INNER_RADIUS);
    expect_color("nested", &pixels, pixel_of(inside), green);
    expect_clear("nested", &pixels, pixel_of(outside));
    expect_color("nested", &pixels, pixel_of((inner.center().x, inner.center().y)), green);
    expect_clear("nested", &pixels, pixel_of((outer.min_x() + OUTER_RADIUS, outer.max_y() - OUTER_RADIUS)));
    expect_clips("nested", clips, 2);
    println!("ok: nested rounded clips keep the inner's inside and cut its corner and the outer's rest");

    // 6. A clip under perspective.
    let m = flip();
    let k = card();
    let (pixels, clips) = render_counted(
      &ctx,
      &list(|b| {
        b.save().transform(&m).clip_rect(&k, ClipOperation::Intersect);
        b.draw_rect(&k.inflate(CARD_OVERDRAW, CARD_OVERDRAW), &solid(GREEN));
        b.restore();
      }),
    );
    save("perspective", &pixels);
    let (left_top, right_top) = (project(&m, k.min_x(), k.min_y()), project(&m, k.max_x(), k.min_y()));
    let (_, right_bottom) = (project(&m, k.min_x(), k.max_y()), project(&m, k.max_x(), k.max_y()));
    expect_color("perspective", &pixels, pixel_of(project(&m, c, c)), green);
    expect_clear("perspective", &pixels, pixel_of((right_top.0 + OUTSIDE_PROBE, c)));
    // The shorter projected side's top sits below the box's top: between
    // them, just inside that side, is in the box and outside the card.
    let (short, long) = if right_bottom.1 - right_top.1 < project(&m, k.min_x(), k.max_y()).1 - left_top.1 {
      (right_top, left_top)
    } else {
      (left_top, right_top)
    };
    let inward = if short.0 > long.0 { -OUTSIDE_PROBE } else { OUTSIDE_PROBE };
    expect_clear("perspective", &pixels, pixel_of((short.0 + inward, (short.1 + long.1) / 2.0)));
    expect_clips("perspective", clips, 2);
    println!("ok: a clip under perspective keeps its projected card and cuts its bounding box's corner");

    // 7. A per-instance rounded clip above a perspective draw.
    let device = rect(DEVICE_CLIP);
    let (pixels, clips) = render_counted(
      &ctx,
      &list(|b| {
        b.clip_rounded_rect(&device, &RoundingRadii::circular([DEVICE_RADIUS; 4]), ClipOperation::Intersect)
          .transform(&m)
          .draw_rect(&k, &solid(GREEN));
      }),
    );
    save("device_rounded", &pixels);
    let (inside, outside) = arc_probes(&device, DEVICE_RADIUS);
    expect_color("device rounded", &pixels, pixel_of(inside), green);
    expect_clear("device rounded", &pixels, pixel_of(outside));
    expect_color("device rounded", &pixels, pixel_of((device.center().x, device.center().y)), green);
    expect_clips("device rounded", clips, 0);
    println!("ok: an axis-aligned rounded clip above a perspective draw stays per instance (0 clip draws)");

    println!("all clip cases verified");
    std::process::exit(0);
  });
}
