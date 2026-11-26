//! What a Paint does to `draw_texture_rect`, which is what the texture
//! element's PaintState can and cannot express.
//!
//! Verified here, on the real GL path:
//!   - the paint's RGB is ignored; only its alpha reaches the draw, as an
//!     opacity multiplier. This is what makes PaintState safe to adopt at all:
//!     PaintState::default() carries grey 0.5, and it leaves pixels untouched.
//!   - a gradient color source is ignored (the texture is the source).
//!   - drawStyle and the stroke fields are ignored.
//!   - blendMode reaches the draw and composites against the target.
//!   - texture alpha is premultiplied, so additive blending is correct with no
//!     manual premultiplication in app code.
//!
//! Renders a source texture, draws it nine ways into a target texture, reads
//! the pixels back and asserts every cell.
//!
//! Run: cargo run -p alloy --example texture_paint

use alloy::renderer::prelude::*;
use alloy::renderer::{
  BlendMode, Color, ColorSource, DisplayList, DisplayListBuilder, DrawStyle, ISize, Paint, Point, Rect, Size,
  TextureSampling, TileMode,
};

const SRC: u32 = 64;
// Half alpha in unorm8, and the one level a GPU's tie rounding may leave
// it off by.
const HALF_LEVEL: i32 = 128;
const HALF_TOLERANCE: i32 = 1;
const TARGET: u32 = 256;
const CELL: f32 = 64.0;

fn fill(color: Color) -> Paint {
  let mut paint = Paint::default();
  paint.set_color(color);
  paint
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
  Rect::new(Point::new(x, y), Size::new(w, h))
}

const WHITE: Color = Color { red: 1.0, green: 1.0, blue: 1.0, alpha: 1.0 };
const BLUE: Color = Color { red: 0.0, green: 0.0, blue: 1.0, alpha: 1.0 };
const RED: Color = Color { red: 1.0, green: 0.0, blue: 0.0, alpha: 1.0 };
const GREEN: Color = Color { red: 0.0, green: 1.0, blue: 0.0, alpha: 1.0 };
const BLACK: Color = Color { red: 0.0, green: 0.0, blue: 0.0, alpha: 1.0 };

// 64x64 in quadrants: opaque white, opaque blue, half-alpha white, undrawn.
// The half-alpha quadrant is the premultiplication probe - what the readback
// reports there tells us how alpha is stored before any blending happens.
fn source_content() -> DisplayList {
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&rect(0.0, 0.0, 32.0, 32.0), &fill(WHITE));
  b.draw_rect(&rect(32.0, 0.0, 32.0, 32.0), &fill(BLUE));
  b.draw_rect(&rect(0.0, 32.0, 32.0, 32.0), &fill(Color { alpha: 0.5, ..WHITE }));
  b.build().expect("build source display list")
}

// Source quadrants, as origins into the texture above.
const Q_WHITE: (f32, f32) = (0.0, 0.0);
const Q_BLUE: (f32, f32) = (32.0, 0.0);
const Q_HALF: (f32, f32) = (0.0, 32.0);

struct Case {
  what: &'static str,
  quadrant: (f32, f32),
  backdrop: Color,
  // `None` draws with no paint at all, the reference every "ignored" case is
  // measured against.
  paint: fn() -> Option<Paint>,
  expected: (u8, u8, u8, u8),
  why: &'static str,
}

const CASES: [Case; 9] = [
  Case {
    what: "no paint",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || None,
    expected: (255, 255, 255, 255),
    why: "reference: the source drawn untouched",
  },
  Case {
    what: "Paint::default()",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || Some(Paint::default()),
    expected: (255, 255, 255, 255),
    why: "a default paint is indistinguishable from no paint",
  },
  Case {
    what: "color RED opaque",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || Some(fill(RED)),
    expected: (255, 255, 255, 255),
    why: "the paint's RGB does not tint the texture",
  },
  Case {
    what: "color grey 0.5 (PaintState::default)",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || Some(fill(Color { red: 0.5, green: 0.5, blue: 0.5, ..WHITE })),
    expected: (255, 255, 255, 255),
    why: "PaintState's default color is inert here, so adopting it changes nothing",
  },
  Case {
    what: "color white alpha 0.5",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || Some(fill(Color { alpha: 0.5, ..WHITE })),
    expected: (128, 128, 128, 255),
    why: "alpha is the one channel that reaches the draw: it multiplies opacity",
  },
  Case {
    what: "gradient color source (red -> green)",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || {
      let mut p = Paint::default();
      let source = ColorSource::linear_gradient(
        Point::new(0.0, 0.0),
        Point::new(64.0, 0.0),
        &[RED, GREEN],
        &[0.0, 1.0],
        TileMode::Clamp,
        None,
      );
      p.set_color_source(&source);
      Some(p)
    },
    expected: (255, 255, 255, 255),
    why: "a color source does not replace the texture",
  },
  Case {
    what: "drawStyle Stroke, strokeWidth 8",
    quadrant: Q_WHITE,
    backdrop: BLACK,
    paint: || {
      let mut p = Paint::default();
      p.set_draw_style(DrawStyle::Stroke);
      p.set_stroke_width(8.0);
      Some(p)
    },
    expected: (255, 255, 255, 255),
    why: "the stroke fields have no meaning for a raster draw and are ignored",
  },
  Case {
    what: "blendMode Plus, opaque blue over red",
    quadrant: Q_BLUE,
    backdrop: RED,
    paint: || {
      let mut p = Paint::default();
      p.set_blend_mode(BlendMode::Plus);
      Some(p)
    },
    expected: (255, 0, 255, 255),
    why: "blendMode reaches the draw: blue + red saturates to magenta",
  },
  Case {
    what: "blendMode Plus, half-alpha white over red",
    quadrant: Q_HALF,
    backdrop: RED,
    paint: || {
      let mut p = Paint::default();
      p.set_blend_mode(BlendMode::Plus);
      Some(p)
    },
    expected: (255, 128, 128, 255),
    why: "source alpha is premultiplied, so a 50% layer adds 50% of its color",
  },
];

fn check(pixels: &[u8], index: usize, case: &Case) {
  let (col, row) = (index % 4, index / 4);
  let (x, y) = (col as u32 * 64 + 32, row as u32 * 64 + 32);
  let i = ((y * TARGET + x) * 4) as usize;
  let (r, g, b, a) = (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]);
  let (er, eg, eb, ea) = case.expected;
  let close = |actual: u8, expected: u8| (actual as i32 - expected as i32).abs() <= 2;
  assert!(
    close(r, er) && close(g, eg) && close(b, eb) && close(a, ea),
    "{}: got ({r},{g},{b},{a}), expected {:?} - {}",
    case.what,
    case.expected,
    case.why
  );
  println!("ok: {:20} ({r:3},{g:3},{b:3},{a:3})  {}", case.what, case.why);
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "texture paint probe", ISize::new(400, 300), alloy::Mode::Run);
  let _ = app.run(|ctx, _cmd_tx, _event_rx| {
    let source = ctx.render_display_list_to_texture(&source_content(), SRC, SRC, true).expect("source rasterization");

    let src_pixels = ctx.read_texture(&source, SRC, SRC).expect("source readback");
    let at = |x: u32, y: u32| {
      let i = ((y * SRC + x) * 4) as usize;
      (src_pixels[i], src_pixels[i + 1], src_pixels[i + 2], src_pixels[i + 3])
    };
    // Premultiplied storage is what makes the additive cases below come out
    // right; assert it directly rather than inferring it from the blend.
    assert_eq!(at(16, 16), (255, 255, 255, 255), "opaque white quadrant");
    assert_eq!(at(48, 16), (0, 0, 255, 255), "opaque blue quadrant");
    // Half of 255 has no exact unorm8: a GPU stores 127 or 128 for it, so
    // this check takes a level of tolerance, as the renders do.
    let half = at(16, 48);
    for channel in [half.0, half.1, half.2, half.3] {
      assert!(
        (channel as i32 - HALF_LEVEL).abs() <= HALF_TOLERANCE,
        "half-alpha white is stored premultiplied: got {half:?}, expected about {HALF_LEVEL} each"
      );
    }
    assert_eq!(at(48, 48), (0, 0, 0, 0), "undrawn quadrant is transparent");
    println!("ok: source texture alpha is premultiplied");

    let mut b = DisplayListBuilder::new(None);
    for (index, case) in CASES.iter().enumerate() {
      let (col, row) = ((index % 4) as f32, (index / 4) as f32);
      let cell = rect(col * CELL, row * CELL, CELL, CELL);
      b.draw_rect(&cell, &fill(case.backdrop));
      let src = rect(case.quadrant.0, case.quadrant.1, 32.0, 32.0);
      b.draw_texture_rect(&source, &src, &cell, TextureSampling::Linear, (case.paint)().as_ref());
    }
    let target = b.build().expect("build target display list");

    let texture = ctx.render_display_list_to_texture(&target, TARGET, TARGET, true).expect("target rasterization");
    let pixels = ctx.read_texture(&texture, TARGET, TARGET).expect("target readback");
    for (index, case) in CASES.iter().enumerate() {
      check(&pixels, index, case);
    }

    image::save_buffer("/tmp/texture_paint.png", &pixels, TARGET, TARGET, image::ColorType::Rgba8).expect("save png");
    println!("saved /tmp/texture_paint.png; all paint cases verified");
    std::process::exit(0);
  });
}
