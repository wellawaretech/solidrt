//! Validates the glyph run op (okf/plans/own-renderer.md, stage 2, text
//! straight into the frame): a word's mask cells from the engine's
//! rasterizer packed into an atlas texture, recorded as `draw_glyphs` into
//! a display list, drawn by anvil's glyph program into an offscreen frame,
//! read back and compared pixel for pixel with the same cells composited on
//! the CPU - which checks the orientation, the atlas mapping and the
//! premultiplied blend at once. Then a half-transparent red run over it,
//! a gradient run whose color follows the ramp across the word, and every
//! coverage mode against the glyph pass (gl/glyphs.rs) it was ported from.
//!
//! Run: cargo run -p alloy --example glyph_run

use alloy::renderer::prelude::*;
use alloy::renderer::{DisplayListBuilder, ISize, Texture, TileMode};
use alloy::rendertree::text::glyphs::{
  AtlasPacker, CellKind, CellRequest, Fallback, FontSet, GlyphAtlas, Hint, Rasterizer, ShapeStyle, ShapedGlyphs,
};
use alloy::rendertree::FontPayload;
use alloy::{
  CoverageMode, CoveragePolicy, GlyphGradient, GlyphGroup, GlyphQuad, GradientKind, SamplerState, RAMP_WIDTH,
};
use std::borrow::Cow;
use std::sync::Arc;

const NOTO_SANS: &[u8] = include_bytes!("../assets/fonts/NotoSans.ttf");
const SIZE: f32 = 32.0;
const WEIGHT: u16 = 500;
const WORD: &str = "Hello";
const FRAME_W: u32 = 192;
const FRAME_H: u32 = 48;
const PEN_X: f32 = 8.0;
const BASELINE_Y: f32 = 36.0;
// Where the second, red run starts: over the first word's tail.
const RED_PEN_X: f32 = 48.0;
// The GPU blends in 8 bits per quad while the CPU composite stays in
// floats, so overlapping runs may differ by a rounding step or two.
const TOLERANCE: i32 = 3;
// The sRGB-ish gamma the remap modes were tuned around, and DirectWrite's
// Windows default.
const REMAP_GAMMA: f32 = 2.2;
const DWRITE_GAMMA: f32 = 1.8;
// The frame is drawn multisampled, as a window frame is.
const AA: bool = true;
// A pixel counts as inked for the gradient read when its alpha is at least
// this (the solid interior of a stem, not an edge).
const INK_MIN: u8 = 200;
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const HALF_RED: [f32; 4] = [1.0, 0.0, 0.0, 0.5];

fn word_quads(
  shaped: &ShapedGlyphs,
  atlas: &GlyphAtlas<u16>,
  pen_x: f32,
  baseline: f32,
  color: [f32; 4],
) -> Vec<GlyphQuad> {
  let mut quads = Vec::new();
  for g in &shaped.glyphs {
    let Some(p) = atlas.packer().placement(g.id) else { continue };
    if p.width == 0 {
      continue;
    }
    let x = (pen_x + g.x).floor() + p.left as f32;
    let y = (baseline + g.y).round() - p.top as f32;
    quads.push(GlyphQuad {
      dst: [x, y, p.width as f32, p.height as f32],
      src: [p.x as f32, p.y as f32, p.width as f32, p.height as f32],
      color,
    });
  }
  quads
}

// The quads composited on the CPU: premultiplied source-over in floats.
fn composite(quads: &[GlyphQuad], packer: &AtlasPacker<u16>) -> Vec<f32> {
  let (atlas_w, _) = packer.size();
  let mirror = packer.mirror();
  let mut out = vec![0f32; (FRAME_W * FRAME_H) as usize * 4];
  for q in quads {
    let (dx, dy, w, h) = (q.dst[0] as i32, q.dst[1] as i32, q.dst[2] as u32, q.dst[3] as u32);
    let (sx, sy) = (q.src[0] as u32, q.src[1] as u32);
    for j in 0..h {
      for i in 0..w {
        let (x, y) = (dx + i as i32, dy + j as i32);
        if x < 0 || y < 0 || x >= FRAME_W as i32 || y >= FRAME_H as i32 {
          continue;
        }
        // A mask atlas holds one coverage byte per texel.
        let texel = ((sy + j) * atlas_w + sx + i) as usize;
        let coverage = mirror[texel] as f32 / 255.0;
        let a = coverage * q.color[3];
        let src = [q.color[0] * a, q.color[1] * a, q.color[2] * a, a];
        let px = (y as u32 * FRAME_W + x as u32) as usize * 4;
        for k in 0..4 {
          out[px + k] = src[k] + out[px + k] * (1.0 - a);
        }
      }
    }
  }
  out
}

// The first pixel channel further than TOLERANCE from `expected`, as a
// message; None when every channel is within it.
fn mismatch(pixels: &[u8], expected: impl Iterator<Item = i32>) -> Option<String> {
  for (i, (&got, want)) in pixels.iter().zip(expected).enumerate() {
    if (got as i32 - want).abs() > TOLERANCE {
      let px = i as u32 / 4;
      return Some(format!("pixel ({}, {}) channel {}: got {got}, expected {want}", px % FRAME_W, px / FRAME_W, i % 4));
    }
  }
  None
}

fn alpha_sum(pixels: &[u8]) -> u64 {
  pixels.chunks(4).map(|p| p[3] as u64).sum()
}

/// The checks run so far and the ones that failed.
#[derive(Default)]
struct Report {
  failed: Vec<String>,
}

impl Report {
  fn check(&mut self, what: &str, failure: Option<String>) {
    match failure {
      None => println!("ok: {what}"),
      Some(why) => {
        println!("FAIL: {what}: {why}");
        self.failed.push(what.to_string());
      }
    }
  }
}

// `runs` recorded as one draw_glyphs each into a frame, drawn offscreen
// and read back, rows top first.
fn draw_runs(ctx: &alloy::Context, atlas: &Texture, runs: &[GlyphGroup], policy: CoveragePolicy) -> Vec<u8> {
  let mut b = DisplayListBuilder::new(None);
  for run in runs {
    // The contract takes the run shared, as a text keeps it.
    let quads: Arc<[GlyphQuad]> = Arc::from(run.quads.as_slice());
    let gradient = run.gradient.clone().map(Arc::new);
    b.draw_glyphs(atlas, &quads, policy, gradient.as_ref());
  }
  let dl = b.build().expect("a builder always builds a list");
  let frame = ctx.render_display_list_to_texture(&dl, FRAME_W, FRAME_H, AA).expect("frame rasterization");
  ctx.read_texture(&frame, FRAME_W, FRAME_H).expect("frame readback")
}

fn solid(quads: Vec<GlyphQuad>) -> GlyphGroup {
  GlyphGroup { quads, gradient: None }
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "glyph run", ISize::new(640, 240), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let fonts = FontSet::from_payloads(
      &[FontPayload { alias: Some("sans".to_string()), bytes: Cow::Borrowed(NOTO_SANS) }],
      |alias, e| panic!("{alias}: {e}"),
    );
    let face = fonts.face(0).expect("the shipped face");
    let style = ShapeStyle { weight: WEIGHT, stretch: 100.0, size: SIZE, line_height: 0.0, letter_spacing: 0.0 };
    let shaped = ShapedGlyphs::shape(&fonts, 0, WORD, &style, Fallback::None).expect("shaped");
    let request = CellRequest {
      kind: CellKind::Mask { ppem: SIZE },
      weight: face.weight_setting(WEIGHT),
      width: None,
      synthetic_bold: face.synthetic_bold(WEIGHT),
      synthetic_italic: false,
      phase: 0.0,
      darken: 0.0,
      hint: Hint::Off,
      glyphs: shaped.glyphs.iter().map(|g| g.id).collect(),
    };
    let cells = Rasterizer::default().rasterize(face.bytes(), face.styles(), &request).expect("a font");
    let mut atlas = GlyphAtlas::new(
      &ctx,
      AtlasPacker::new(CellKind::Mask { ppem: SIZE }, 2048),
      SamplerState::default(),
      "glyph-run-example",
    )
    .expect("atlas texture");
    for cell in cells {
      atlas.insert(cell);
    }
    atlas.flush(&ctx).expect("atlas upload");
    // The handle a recording draws the atlas through, read after the flush.
    let handle = ctx
      .textures
      .get(atlas.texture())
      .and_then(|entry| entry.handle.clone())
      .expect("the atlas has a renderer handle");
    let mut report = Report::default();

    // A white word under naive coverage: every pixel against the CPU
    // composite.
    let white = word_quads(&shaped, &atlas, PEN_X, BASELINE_Y, WHITE);
    assert!(!white.is_empty(), "the word has glyph quads");
    let naive = CoveragePolicy { mode: CoverageMode::Naive, gamma: REMAP_GAMMA, ..CoveragePolicy::default() };
    let white_pixels = draw_runs(&ctx, &handle, &[solid(white.clone())], naive);
    report.check("the white word has ink", (alpha_sum(&white_pixels) == 0).then(|| "no ink".to_string()));
    let expected = composite(&white, atlas.packer());
    report.check(
      "white word, naive, matches the CPU composite",
      mismatch(&white_pixels, expected.iter().map(|v| (v * 255.0).round() as i32)),
    );

    // A half-transparent red run over its tail, as a second op: the two
    // blend source-over.
    let red = word_quads(&shaped, &atlas, RED_PEN_X, BASELINE_Y, HALF_RED);
    let both_pixels = draw_runs(&ctx, &handle, &[solid(white.clone()), solid(red.clone())], naive);
    let mut both = white.clone();
    both.extend(red);
    let expected = composite(&both, atlas.packer());
    report.check(
      "white and half red, naive, matches the CPU composite",
      mismatch(&both_pixels, expected.iter().map(|v| (v * 255.0).round() as i32)),
    );

    // A gradient run: red at the word's pen to blue at its advance, so its
    // first stem reads red and its last reads blue, and the coverage still
    // shapes the glyphs (the same alpha as the white word).
    let ramp: Vec<[u8; 4]> = (0..RAMP_WIDTH)
      .map(|i| {
        let t = i as f32 / (RAMP_WIDTH - 1) as f32;
        [((1.0 - t) * 255.0).round() as u8, 0, (t * 255.0).round() as u8, 255]
      })
      .collect();
    let advance = shaped.metrics.advance;
    let gradient = GlyphGradient {
      kind: GradientKind::Linear,
      x_row: [1.0 / advance, 0.0, -PEN_X / advance],
      y_row: [0.0; 3],
      tile: TileMode::Clamp,
      ramp,
    };
    let graded = GlyphGroup { quads: white.clone(), gradient: Some(gradient) };
    let graded_pixels = draw_runs(&ctx, &handle, std::slice::from_ref(&graded), naive);
    let alphas = |pixels: &[u8]| pixels.chunks(4).map(|p| p[3]).collect::<Vec<u8>>();
    report.check(
      "the gradient changes color, not coverage",
      (alphas(&graded_pixels) != alphas(&white_pixels)).then(|| "the alphas differ".to_string()),
    );
    let inked =
      |x: u32| (0..FRAME_H).map(|y| ((y * FRAME_W + x) as usize) * 4).find(|&i| graded_pixels[i + 3] >= INK_MIN);
    let left = (0..FRAME_W).find_map(inked).expect("an inked column on the left");
    let right = (0..FRAME_W).rev().find_map(inked).expect("an inked column on the right");
    let (l, r) = (&graded_pixels[left..left + 3], &graded_pixels[right..right + 3]);
    report.check(
      "a gradient run reads red to blue across the word",
      (l[0] <= l[2] || r[2] <= r[0]).then(|| format!("first stem {l:?}, last stem {r:?}")),
    );

    // Every coverage mode, white text and black, against the glyph pass:
    // its layer is drawn at the same pixels, so the two agree texel for
    // texel within the blend's rounding. A gradient run under the default
    // policy too.
    let black = word_quads(&shaped, &atlas, PEN_X, BASELINE_Y, BLACK);
    let modes = [
      ("naive", CoverageMode::Naive, REMAP_GAMMA),
      ("linear light", CoverageMode::LinearLight, REMAP_GAMMA),
      ("polarity remap", CoverageMode::PolarityRemap, REMAP_GAMMA),
      ("polarity linear", CoverageMode::PolarityLinear, REMAP_GAMMA),
      ("directwrite", CoverageMode::DirectWrite, DWRITE_GAMMA),
    ];
    let mut cases = Vec::new();
    for (label, mode, gamma) in modes {
      let policy = CoveragePolicy { mode, gamma, ..CoveragePolicy::default() };
      cases.push((format!("white, {label}"), solid(white.clone()), policy));
      cases.push((format!("black, {label}"), solid(black.clone()), policy));
    }
    cases.push(("gradient, directwrite".to_string(), graded, CoveragePolicy::default()));
    for (label, run, policy) in cases {
      let ours = draw_runs(&ctx, &handle, std::slice::from_ref(&run), policy);
      let layer = ctx.rasterize_glyphs(vec![run], atlas.texture(), FRAME_W, FRAME_H, policy, None).expect("glyph pass");
      let theirs = ctx.read_texture(&layer, FRAME_W, FRAME_H).expect("layer readback");
      report.check(&format!("{label} matches the glyph pass"), mismatch(&ours, theirs.iter().map(|&v| v as i32)));
    }

    atlas.destroy(&ctx);
    if !report.failed.is_empty() {
      println!("glyph run example: {} checks failed", report.failed.len());
      std::process::exit(1);
    }
    println!("glyph run example: all checks passed");
  });
}
