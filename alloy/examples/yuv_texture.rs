//! Planar YUV textures on the real GL path: an NV12 texture converts a
//! synthetic quadrant frame to the reference RGB values (pinning matrix,
//! range handling, and orientation), a second push through the frame sink
//! re-renders the conversion target, a frame due later than the frame's
//! deadline stays queued, I420 tri-plane input lands the same colors as
//! NV12, full range skips the studio-swing expansion, and destroy takes the
//! planes down with the output id.
//!
//! Run: cargo run -p alloy --example yuv_texture

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use alloy::renderer::ISize;
use alloy::yuv::coefficients;
use alloy::{SamplerState, YuvLayout, YuvMatrix, YuvRange};

// A due time no frame's deadline reaches during the probe.
const FAR_FUTURE_NS: i64 = i64::MAX / 2;

const W: u32 = 64;
const H: u32 = 64;

// Studio-range quadrant colors as (Y, U, V): white, red, blue, black.
const QUADS: [(u8, u8, u8); 4] = [(235, 128, 128), (81, 90, 240), (41, 240, 110), (16, 128, 128)];

// Quadrant of a pixel: 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right.
// Row 0 of the uploaded planes is the TOP of the displayed texture, so the
// corner asserts below pin orientation, not just conversion.
fn quad_at(x: usize, y: usize, w: usize, h: usize) -> usize {
  (x >= w / 2) as usize + 2 * ((y >= h / 2) as usize)
}

fn nv12_frame(w: usize, h: usize, quads: [(u8, u8, u8); 4]) -> Vec<u8> {
  let (cw, ch) = (w / 2, h / 2);
  let mut f = vec![0u8; w * h + cw * ch * 2];
  for y in 0..h {
    for x in 0..w {
      f[y * w + x] = quads[quad_at(x, y, w, h)].0;
    }
  }
  for y in 0..ch {
    for x in 0..cw {
      let (_, u, v) = quads[quad_at(x * 2, y * 2, w, h)];
      f[w * h + (y * cw + x) * 2] = u;
      f[w * h + (y * cw + x) * 2 + 1] = v;
    }
  }
  f
}

fn i420_frame(w: usize, h: usize, quads: [(u8, u8, u8); 4]) -> Vec<u8> {
  let (cw, ch) = (w / 2, h / 2);
  let mut f = vec![0u8; w * h + cw * ch * 2];
  for y in 0..h {
    for x in 0..w {
      f[y * w + x] = quads[quad_at(x, y, w, h)].0;
    }
  }
  for y in 0..ch {
    for x in 0..cw {
      let (_, u, v) = quads[quad_at(x * 2, y * 2, w, h)];
      f[w * h + y * cw + x] = u;
      f[w * h + cw * ch + y * cw + x] = v;
    }
  }
  f
}

// The reference conversion, from the same coefficients the shader bakes in.
fn ref_rgb(matrix: YuvMatrix, range: YuvRange, (y, u, v): (u8, u8, u8)) -> (u8, u8, u8) {
  let [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u] = coefficients(matrix, range);
  let yf = (y as f32 / 255.0 - y_offset) * y_scale;
  let cb = (u as f32 / 255.0 - 128.0 / 255.0) * c_scale;
  let cr = (v as f32 / 255.0 - 128.0 / 255.0) * c_scale;
  let to = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
  (to(yf + r_v * cr), to(yf + g_u * cb + g_v * cr), to(yf + b_u * cb))
}

fn expect_color(pixels: &[u8], x: u32, y: u32, expected: (u8, u8, u8), what: &str) {
  let i = ((y * W + x) * 4) as usize;
  let (r, g, b, a) = (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]);
  let close = |actual: u8, exp: u8| (actual as i32 - exp as i32).abs() <= 3;
  assert!(
    close(r, expected.0) && close(g, expected.1) && close(b, expected.2) && a == 255,
    "{what} at ({x},{y}): got ({r},{g},{b},{a}), expected {expected:?}"
  );
  println!("ok: {what} at ({x},{y}) = ({r},{g},{b})");
}

// Quadrant center sample points (clear of the chroma boundary blend).
const CENTERS: [(u32, u32); 4] = [(W / 4, H / 4), (3 * W / 4, H / 4), (W / 4, 3 * H / 4), (3 * W / 4, 3 * H / 4)];

fn expect_quads(pixels: &[u8], matrix: YuvMatrix, range: YuvRange, quads: [(u8, u8, u8); 4], what: &str) {
  for (q, &(x, y)) in CENTERS.iter().enumerate() {
    expect_color(pixels, x, y, ref_rgb(matrix, range, quads[q]), &format!("{what} quadrant {q}"));
  }
}

fn main() {
  let platform = ore::platform::Platform::init(ore::platform::PlatformConfig { app_id: "solidrt", headless: false })
    .expect("Failed to initialize the platform");
  let app = alloy::setup(platform, "yuv texture probe", ISize::new(400, 300), alloy::Mode::Run);
  let _ = app.run(|ctx, _cmd_tx, _event_rx| {
    // NV12, BT.601 limited: seeded black, then the quadrant frame.
    let nv12 = ctx
      .create_yuv_texture(
        W,
        H,
        YuvLayout::Nv12,
        YuvMatrix::Bt601,
        YuvRange::Limited,
        SamplerState::default(),
        Some("probe-nv12".into()),
      )
      .expect("create nv12");
    let (_, _, pixels) = ctx.read_texture_by_id(nv12).expect("readback seeded");
    expect_color(&pixels, W / 4, H / 4, (0, 0, 0), "seeded content is black");

    // Frames reach the texture through its sink; a due time of zero latches
    // at the next frame, which the readback's frame is.
    let request = Arc::new(AtomicBool::new(false));
    let sink = ctx.yuv_frame_sink(nv12, request.clone()).expect("frame sink");
    sink.push(nv12_frame(W as usize, H as usize, QUADS), 0, 0).expect("push nv12");
    assert!(request.load(std::sync::atomic::Ordering::Relaxed), "a push requests a frame");
    let (_, _, pixels) = ctx.read_texture_by_id(nv12).expect("readback nv12");
    expect_quads(&pixels, YuvMatrix::Bt601, YuvRange::Limited, QUADS, "nv12 bt601");
    assert_eq!(sink.shown_pts_us(), Some(0), "the latched frame is the shown one");

    // A second push re-renders the conversion (quadrants rotated one step).
    let rotated = [QUADS[1], QUADS[3], QUADS[0], QUADS[2]];
    sink.push(nv12_frame(W as usize, H as usize, rotated), 40_000, 0).expect("push rotated");
    let (_, _, pixels) = ctx.read_texture_by_id(nv12).expect("readback rotated");
    expect_quads(&pixels, YuvMatrix::Bt601, YuvRange::Limited, rotated, "nv12 update");
    assert_eq!(sink.shown_pts_us(), Some(40_000));

    // A frame due far in the future is not latched: the picture holds.
    sink.push(nv12_frame(W as usize, H as usize, QUADS), 80_000, FAR_FUTURE_NS).expect("push future");
    let (_, _, pixels) = ctx.read_texture_by_id(nv12).expect("readback held");
    expect_quads(&pixels, YuvMatrix::Bt601, YuvRange::Limited, rotated, "frame due later stays queued");
    assert_eq!(sink.shown_pts_us(), Some(40_000));

    // I420 tri-plane input, same content, same colors.
    let i420 = ctx
      .create_yuv_texture(
        W,
        H,
        YuvLayout::I420,
        YuvMatrix::Bt601,
        YuvRange::Limited,
        SamplerState::default(),
        Some("probe-i420".into()),
      )
      .expect("create i420");
    let i420_sink = ctx.yuv_frame_sink(i420, request.clone()).expect("frame sink");
    i420_sink.push(i420_frame(W as usize, H as usize, QUADS), 0, 0).expect("push i420");
    let (_, _, pixels) = ctx.read_texture_by_id(i420).expect("readback i420");
    expect_quads(&pixels, YuvMatrix::Bt601, YuvRange::Limited, QUADS, "i420 bt601");

    // Full-range BT.709: values pass through without studio-swing expansion.
    let full_quads = [(255, 128, 128), (76, 85, 255), (29, 255, 107), (0, 128, 128)];
    let full = ctx
      .create_yuv_texture(
        W,
        H,
        YuvLayout::Nv12,
        YuvMatrix::Bt709,
        YuvRange::Full,
        SamplerState::default(),
        Some("probe-full".into()),
      )
      .expect("create full-range");
    let full_sink = ctx.yuv_frame_sink(full, request.clone()).expect("frame sink");
    full_sink.push(nv12_frame(W as usize, H as usize, full_quads), 0, 0).expect("push full-range");
    let (_, _, pixels) = ctx.read_texture_by_id(full).expect("readback full-range");
    expect_quads(&pixels, YuvMatrix::Bt709, YuvRange::Full, full_quads, "nv12 bt709 full");

    // Call-site validation: a short frame and an unknown id both err.
    let err = sink.push(vec![0u8; 16], 0, 0).expect_err("short frame must err");
    assert!(err.contains("need") && err.contains("16"), "unexpected error: {err}");
    println!("ok: short frame errs ({err})");
    let err = ctx.yuv_frame_sink(9999, request.clone()).err().expect("unknown id must err");
    assert!(err.contains("not found"), "unexpected error: {err}");
    println!("ok: unknown id errs ({err})");

    // Destroy closes the latch: a late push is dropped, and nothing dirties
    // a target whose planes are going away.
    ctx.destroy_texture(nv12);
    sink.push(nv12_frame(W as usize, H as usize, QUADS), 120_000, 0).expect("a push after destroy is a no-op");
    assert_eq!(sink.shown_pts_us(), Some(40_000), "nothing latched after destroy");
    println!("ok: push after destroy is dropped");

    // A PCM sink's handle outlives its registry entry: the stream goes with
    // the last holder, on whatever thread that is.
    if let Ok(pcm) = ctx.create_pcm_sink(48_000, 1, None) {
      let mut handle = ctx.pcm_sink_handle(pcm).expect("pcm handle");
      ctx.destroy_pcm_sink(pcm);
      let pushed = std::thread::spawn(move || {
        handle.push(&[0.0f32; 480]).expect("a push on a destroyed sink's handle still lands");
        handle.queued_us()
      })
      .join()
      .expect("handle thread");
      assert!(pushed >= 0);
      println!("ok: pcm handle outlives its registry entry ({pushed}us queued on a paused stream)");
    } else {
      println!("skip: no audio device for the pcm handle check");
    }

    let (_, _, pixels) = ctx.read_texture_by_id(i420).expect("final readback");
    image::save_buffer("/tmp/yuv_texture.png", &pixels, W, H, image::ColorType::Rgba8).expect("save png");
    println!("saved /tmp/yuv_texture.png; NV12/I420 conversion, updates, ranges, and destroy verified");
    std::process::exit(0);
  });
}
