//! A paint shader drawn straight into a list (`Context::create_paint_shader`
//! and `DisplayListBuilder::draw_shader`), on the real GL path, against the
//! same fragment source through a shader texture.
//!
//! Verified here:
//!   - the direct draw equals the shader texture drawn 1:1, every pixel
//!     within one level per channel (the 1/255 rounding class): the same
//!     `vUV` with a top-left origin, `iResolution`, params and a sampled
//!     input, `fragColor` premultiplied;
//!   - params and inputs validate against the reflected uniforms, and the
//!     renderer's own vertex uniform is not settable;
//!   - under a rect clip the clipped half stays transparent;
//!   - under a rounded clip the corner stays transparent;
//!   - under a 45 degree rotation the corner the quad left stays
//!     transparent;
//!   - a retained list recorded before `destroy_paint_shader` still draws:
//!     the list holds the handle.
//!
//! Run: cargo run -p alloy --example paint_shader

use alloy::renderer::prelude::*;
use alloy::renderer::{
  ClipOperation, DisplayList, DisplayListBuilder, ISize, Matrix, PaintShader, Point, Rect, RoundingRadii, ShaderInput,
  ShaderParam, Size, Texture, TextureSampling, TextureWrap,
};
use alloy::{Context, ParamValue, SamplerState, TextureBinding, TextureFormat};
use std::sync::Arc;

// The targets' edge in pixels.
const SIZE: u32 = 128;
// The sampled input's edge in texels: smaller than the target, so the input
// is filtered.
const INPUT: u32 = 16;
// How many levels the input gradient climbs per texel.
const INPUT_STEP: u32 = 16;
// The `uMix` param: how much of the input shows over the split.
const MIX: f32 = 0.35;
// The largest per-channel difference between the two routes: the 1/255
// rounding class.
const TOLERANCE: i32 = 1;
// The rounded clip's corner radius.
const RADIUS: f32 = 32.0;
// The rotation case's angle.
const ROTATION_DEGREES: f32 = 45.0;
// A pixel nothing drew.
const CLEAR: [u8; 4] = [0, 0, 0, 0];

// A soft diagonal split of two colors, faded to partial alpha on one side,
// rippled along x through `iResolution`, with the input mixed over it by
// `uMix`. The split is soft so no pixel sits on a tie the two routes could
// round to different sides.
const SOURCE: &str = r"
uniform float uMix;
uniform sampler2D uTex;
// The width of the soft diagonal edge, in UV.
const float EDGE_WIDTH = 0.25;
// The ripple's period in device pixels, and how deep it darkens.
const float RIPPLE_PERIOD = 8.0;
const float RIPPLE_DEPTH = 0.25;
// The alpha of the faded side.
const float FADED_ALPHA = 0.6;
void main() {
  float side = smoothstep(-EDGE_WIDTH, EDGE_WIDTH, vUV.y - vUV.x);
  vec3 split = mix(vec3(0.9, 0.2, 0.1), vec3(0.1, 0.3, 0.9), side);
  float ripple = 0.5 + 0.5 * sin(vUV.x * iResolution.x / RIPPLE_PERIOD);
  vec3 color = mix(split, texture(uTex, vUV).rgb, uMix) * (1.0 - RIPPLE_DEPTH * ripple);
  float alpha = mix(1.0, FADED_ALPHA, side);
  fragColor = vec4(color * alpha, alpha);
}
";

fn full() -> Rect {
  Rect::new(Point::zero(), Size::new(SIZE as f32, SIZE as f32))
}

// The input: a red-green gradient over x and y, opaque.
fn input_pixels() -> Vec<u8> {
  let mut pixels = Vec::with_capacity((INPUT * INPUT * 4) as usize);
  for y in 0..INPUT {
    for x in 0..INPUT {
      pixels.extend_from_slice(&[(x * INPUT_STEP) as u8, (y * INPUT_STEP) as u8, 128, 255]);
    }
  }
  pixels
}

fn render(ctx: &Context, list: &DisplayList) -> Vec<u8> {
  let texture = ctx.render_display_list_to_texture(list, SIZE, SIZE, true).expect("rasterize the list");
  ctx.read_texture(&texture, SIZE, SIZE).expect("read the target back")
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
  let i = ((y * SIZE + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
  a.iter().zip(b.iter()).all(|(a, b)| (*a as i32 - *b as i32).abs() <= TOLERANCE)
}

fn save(name: &str, pixels: &[u8]) {
  let path = format!("/tmp/paint_shader_{name}.png");
  image::save_buffer(&path, pixels, SIZE, SIZE, image::ColorType::Rgba8).expect("save png");
  println!("saved {path}");
}

// One list: `setup` puts the state, then the paint shader draws over the
// whole target.
fn direct(
  shader: &PaintShader,
  params: &Arc<[ShaderParam]>,
  inputs: &Arc<[ShaderInput]>,
  setup: impl FnOnce(&mut DisplayListBuilder),
) -> DisplayList {
  let mut b = DisplayListBuilder::new(None);
  b.save();
  setup(&mut b);
  b.draw_shader(&full(), shader, params, inputs);
  b.restore();
  b.build().expect("build the direct list")
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "paint shader probe", ISize::new(400, 300), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let input_id = ctx
      .create_texture_from_pixels(
        INPUT,
        INPUT,
        &input_pixels(),
        SamplerState::default(),
        TextureFormat::Rgba8,
        Some("input".into()),
      )
      .expect("create the input");
    let input: Texture =
      ctx.textures.get(input_id).and_then(|e| e.handle.clone()).expect("the input has a renderer handle");

    // The reference: the source through a shader texture, drawn 1:1.
    let reference_id = ctx
      .create_shader_texture(
        SIZE,
        SIZE,
        SOURCE,
        &[("uMix".to_string(), ParamValue::Scalar(MIX))],
        &[TextureBinding::new("uTex", input_id)],
        SamplerState::default(),
        Some("reference".into()),
      )
      .expect("create the shader texture");
    let shown = ctx.textures.get(reference_id).and_then(|e| e.handle.clone()).expect("the shader texture's handle");
    let mut b = DisplayListBuilder::new(None);
    b.draw_texture_rect(&shown, &full(), &full(), TextureSampling::Nearest, None);
    let reference = render(&ctx, &b.build().expect("build the reference list"));
    save("reference", &reference);

    // The direct route.
    let paint_id = ctx.create_paint_shader(SOURCE, Some("paint".into())).expect("link the paint shader");
    let params: Arc<[ShaderParam]> = vec![ShaderParam { name: "uMix".into(), value: vec![MIX] }].into();
    let inputs: Arc<[ShaderInput]> = vec![ShaderInput {
      name: "uTex".into(),
      texture: input.clone(),
      sampling: TextureSampling::Linear,
      wrap: TextureWrap::Clamp,
    }]
    .into();

    // Validation against the reflected table.
    let param = |name: &str, value: ParamValue| vec![(name.to_string(), value)];
    ctx.validate_paint_params(paint_id, &param("uMix", ParamValue::Scalar(MIX))).expect("uMix validates");
    assert!(ctx.validate_paint_params(paint_id, &param("uNope", ParamValue::Scalar(1.0))).is_err(), "unknown name");
    assert!(ctx.validate_paint_params(paint_id, &param("uMix", ParamValue::Array(vec![1.0, 2.0]))).is_err(), "arity");
    assert!(
      ctx.validate_paint_params(paint_id, &param("uSize", ParamValue::Array(vec![1.0, 1.0]))).is_err(),
      "the renderer's vertex uniform is not settable"
    );
    let uniforms = ctx.paint_shader_uniforms(paint_id).expect("the table");
    assert!(uniforms.contains_key("iResolution"), "iResolution is in the table: {:?}", uniforms.keys());
    ctx.validate_paint_inputs(paint_id, &[TextureBinding::new("uTex", input_id)]).expect("uTex validates");
    assert!(ctx.validate_paint_inputs(paint_id, &[TextureBinding::new("uMix", input_id)]).is_err(), "not a sampler");
    println!("ok: params and inputs validate against the reflected uniforms; uSize is not settable");
    let listed = ctx.gpu_resources().paint_shaders.into_iter().find(|p| p.id == paint_id).expect("listed");
    assert_eq!(listed.label.as_deref(), Some("paint"));
    println!("ok: the resource inventory lists the paint shader with its label");

    let entry = ctx.paint_shaders.get(paint_id).expect("the registry holds the shader");
    let shader = entry.handle.clone();
    drop(entry);

    // 1. The direct draw against the reference.
    let plain = render(&ctx, &direct(&shader, &params, &inputs, |_| {}));
    save("direct", &plain);
    let mut worst = 0;
    for y in 0..SIZE {
      for x in 0..SIZE {
        let (a, r) = (pixel(&plain, x, y), pixel(&reference, x, y));
        let diff = a.iter().zip(r.iter()).map(|(a, r)| (*a as i32 - *r as i32).abs()).max().unwrap_or(0);
        assert!(diff <= TOLERANCE, "pixel ({x}, {y}): direct {a:?}, reference {r:?}");
        worst = worst.max(diff);
      }
    }
    println!("ok: the direct draw equals the shader texture (largest channel difference {worst})");

    let center = (SIZE / 2, SIZE / 2);
    let painted = |pixels: &[u8], what: &str| {
      let (got, want) = (pixel(pixels, center.0, center.1), pixel(&reference, center.0, center.1));
      assert!(close(got, want), "{what}: center {got:?}, reference {want:?}");
    };

    // 2. A rect clip to the left half.
    let half = SIZE as f32 / 2.0;
    let clipped = render(
      &ctx,
      &direct(&shader, &params, &inputs, |b| {
        b.clip_rect(&Rect::new(Point::zero(), Size::new(half, SIZE as f32)), ClipOperation::Intersect);
      }),
    );
    save("clip_rect", &clipped);
    for y in 0..SIZE {
      for x in SIZE / 2..SIZE {
        assert_eq!(pixel(&clipped, x, y), CLEAR, "pixel ({x}, {y}) is outside the clip");
      }
    }
    let left = (SIZE / 4, SIZE / 2);
    assert!(close(pixel(&clipped, left.0, left.1), pixel(&reference, left.0, left.1)), "the left half is painted");
    println!("ok: under a rect clip the right half is transparent");

    // 3. A rounded clip.
    let rounded = render(
      &ctx,
      &direct(&shader, &params, &inputs, |b| {
        b.clip_rounded_rect(&full(), &RoundingRadii::circular([RADIUS; 4]), ClipOperation::Intersect);
      }),
    );
    save("clip_rounded", &rounded);
    assert_eq!(pixel(&rounded, 1, 1), CLEAR, "the corner is outside the rounding");
    painted(&rounded, "rounded clip");
    println!("ok: under a rounded clip the corner is transparent and the center is painted");

    // 4. A rotation about the center.
    let rotated = render(
      &ctx,
      &direct(&shader, &params, &inputs, |b| {
        let (sin, cos) = ROTATION_DEGREES.to_radians().sin_cos();
        b.translate(half, half);
        b.transform(&Matrix::new_2d(cos, sin, -sin, cos, 0.0, 0.0));
        b.translate(-half, -half);
      }),
    );
    save("rotated", &rotated);
    assert_eq!(pixel(&rotated, 1, 1), CLEAR, "the rotated quad leaves the target's corner");
    // The center pixel samples the pattern a fraction of a pixel away under
    // the rotation, so it is painted, not equal to the reference's.
    let middle = pixel(&rotated, center.0, center.1);
    assert!(middle[3] > 0, "rotation: the center is painted, got {middle:?}");
    println!("ok: under a 45 degree rotation the corner is transparent and the center is painted");

    // 5. A retained list outlives the destroy.
    let retained = direct(&shader, &params, &inputs, |_| {});
    drop(shader);
    ctx.destroy_paint_shader(paint_id);
    assert!(ctx.paint_shaders.get(paint_id).is_none(), "the registry entry is gone");
    assert!(!ctx.gpu_resources().paint_shaders.iter().any(|p| p.id == paint_id), "the inventory no longer lists it");
    let after = render(&ctx, &retained);
    save("retained", &after);
    painted(&after, "retained list");
    println!("ok: a retained list recorded before destroy_paint_shader still paints");

    println!("all paint shader cases verified");
    std::process::exit(0);
  });
}
