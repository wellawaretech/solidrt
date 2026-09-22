use crate::gpu::TextureFormat;
use crate::yuv::{coefficients, fragment_src, frame_size, planes, YuvLayout, YuvMatrix, YuvRange};

#[test]
fn nv12_planes_match_the_probed_decoder_layout() {
  // 1920x1080: Y then interleaved UV at half resolution, tightly packed.
  let p = planes(YuvLayout::Nv12, 1920, 1080);
  assert_eq!(p.len(), 2);
  assert_eq!((p[0].name, p[0].width, p[0].height, p[0].format, p[0].offset), ("uY", 1920, 1080, TextureFormat::R8, 0));
  assert_eq!(
    (p[1].name, p[1].width, p[1].height, p[1].format, p[1].offset),
    ("uUV", 960, 540, TextureFormat::Rg8, 1920 * 1080)
  );
  assert_eq!(frame_size(YuvLayout::Nv12, 1920, 1080), 1920 * 1080 * 3 / 2);
}

#[test]
fn i420_planes_are_y_u_v_in_order() {
  let p = planes(YuvLayout::I420, 64, 48);
  assert_eq!(p.len(), 3);
  assert_eq!((p[0].name, p[0].offset), ("uY", 0));
  assert_eq!((p[1].name, p[1].width, p[1].height, p[1].format), ("uU", 32, 24, TextureFormat::R8));
  assert_eq!(p[1].offset, 64 * 48);
  assert_eq!((p[2].name, p[2].offset), ("uV", 64 * 48 + 32 * 24));
  assert_eq!(frame_size(YuvLayout::I420, 64, 48), 64 * 48 * 3 / 2);
}

#[test]
fn odd_sizes_round_chroma_up() {
  // 5x3: chroma covers 3x2; frame size counts the rounded planes.
  let p = planes(YuvLayout::Nv12, 5, 3);
  assert_eq!((p[1].width, p[1].height), (3, 2));
  assert_eq!(frame_size(YuvLayout::Nv12, 5, 3), 5 * 3 + 3 * 2 * 2);
  let p = planes(YuvLayout::I420, 5, 3);
  assert_eq!((p[1].width, p[1].height, p[2].width, p[2].height), (3, 2, 3, 2));
  assert_eq!(frame_size(YuvLayout::I420, 5, 3), 5 * 3 + 3 * 2 * 2);
}

#[test]
fn coefficients_hit_the_standard_bt601_limited_values() {
  // The textbook BT.601 studio-range constants: R = 1.164(Y-16) + 1.596 Cr' etc.
  // (on 0..255 scales; ours are normalized, the ratios are what is pinned).
  let [y_scale, y_offset, c_scale, r_v, g_u, g_v, b_u] = coefficients(YuvMatrix::Bt601, YuvRange::Limited);
  assert!((y_scale - 255.0 / 219.0).abs() < 1e-6);
  assert!((y_offset - 16.0 / 255.0).abs() < 1e-6);
  assert!((r_v * c_scale - 1.596).abs() < 1e-3);
  assert!((r_v - 1.402).abs() < 1e-3);
  assert!((g_u + 0.344).abs() < 1e-3);
  assert!((g_v + 0.714).abs() < 1e-3);
  assert!((b_u - 1.772).abs() < 1e-3);
}

#[test]
fn full_range_is_identity_scaling() {
  let [y_scale, y_offset, c_scale, ..] = coefficients(YuvMatrix::Bt709, YuvRange::Full);
  assert_eq!((y_scale, y_offset, c_scale), (1.0, 0.0, 1.0));
}

#[test]
fn fragment_src_declares_the_layout_samplers() {
  let nv12 = fragment_src(YuvLayout::Nv12, YuvMatrix::Bt709, YuvRange::Limited);
  assert!(nv12.contains("uniform sampler2D uY;") && nv12.contains("uniform sampler2D uUV;"));
  assert!(!nv12.contains("uU;"));
  let i420 = fragment_src(YuvLayout::I420, YuvMatrix::Bt601, YuvRange::Full);
  assert!(i420.contains("uniform sampler2D uU;") && i420.contains("uniform sampler2D uV;"));
}

// --- The latch (see yuv.rs) ---

use crate::yuv::{LatchedFrame, Pushed, YuvLatch, YuvLatchShared, LATCH_QUEUE_FRAMES};

fn frame(pts_us: i64, due_ns: i64) -> LatchedFrame {
  LatchedFrame { due_ns, pts_us, data: vec![pts_us as u8] }
}

// A 50 Hz grid: 20 ms periods, a 10 ms lookahead.
const PERIOD_NS: i64 = 20_000_000;
const LOOKAHEAD_NS: i64 = PERIOD_NS / 2;

#[test]
fn the_take_hands_back_the_newest_due_frame_and_counts_the_skipped() {
  let mut latch = YuvLatch::new();
  assert!(latch.take(0, LOOKAHEAD_NS).is_none(), "nothing queued, nothing due");
  for k in 0..4 {
    assert_eq!(latch.push(frame(k * 40_000, k * 40_000_000), true), Pushed::Queued);
  }
  // Deadline 45 ms: frames due at 0 and 40 ms are due (40 + 10 lookahead
  // reaches 50); the newest wins and the older one is skipped.
  let taken = latch.take(45_000_000, LOOKAHEAD_NS).expect("a frame is due");
  assert_eq!(taken.frame.pts_us, 40_000);
  assert_eq!(taken.skipped, 1);
  assert_eq!(latch.shown_pts_us(), Some(40_000));
  assert_eq!(latch.len(), 2, "the frames due later stay queued");
  // Nothing due for the next deadline within the lookahead.
  assert!(latch.take(65_000_000, LOOKAHEAD_NS).is_none());
  assert_eq!(latch.shown_pts_us(), Some(40_000), "the shown frame stays what it was");
  // Exactly at the lookahead boundary: due.
  let taken = latch.take(70_000_000, LOOKAHEAD_NS).expect("80 ms is within the lookahead of 70 ms");
  assert_eq!(taken.frame.pts_us, 80_000);
  assert_eq!(taken.skipped, 0);
}

#[test]
fn a_full_latch_evicts_the_oldest_against_a_live_compositor_and_refuses_against_a_stepped_one() {
  let mut latch = YuvLatch::new();
  for k in 0..LATCH_QUEUE_FRAMES as i64 {
    assert_eq!(latch.push(frame(k, k * PERIOD_NS), true), Pushed::Queued);
  }
  assert_eq!(latch.push(frame(99, 99 * PERIOD_NS), false), Pushed::Full, "a stepped push waits instead");
  assert_eq!(latch.len(), LATCH_QUEUE_FRAMES);
  assert_eq!(latch.push(frame(4, 4 * PERIOD_NS), true), Pushed::Evicted);
  assert_eq!(latch.len(), LATCH_QUEUE_FRAMES);
  // The eviction is a skip: it shows in the next take's count, and a take
  // that finds nothing due (a deadline before every queued frame) keeps
  // it for the take that does.
  assert!(latch.take(-PERIOD_NS, 0).is_none());
  let taken = latch.take(4 * PERIOD_NS, LOOKAHEAD_NS).expect("due");
  assert_eq!(taken.frame.pts_us, 4);
  assert_eq!(taken.skipped, 1 + 3, "the evicted frame plus the three older due ones");
}

#[test]
fn a_re_anchored_frame_is_queued_in_due_order() {
  let mut latch = YuvLatch::new();
  latch.push(frame(1, 100_000_000), true);
  latch.push(frame(2, 140_000_000), true);
  // A seek re-anchors: the target frame is due now, ahead of the queued
  // ones; the take for now hands it back and leaves the later ones alone.
  latch.push(frame(3, 50_000_000), true);
  let taken = latch.take(50_000_000, 0).expect("due");
  assert_eq!(taken.frame.pts_us, 3);
  assert_eq!(taken.skipped, 0);
  assert_eq!(latch.len(), 2);
  latch.flush();
  assert!(latch.is_empty());
}

#[test]
fn a_closed_latch_drops_pushes_and_the_peek_marks_a_late_latch() {
  let mut latch = YuvLatch::new();
  // The gate peeked and found nothing due; a frame pushed after the peek
  // for the same deadline is latched late.
  assert!(!latch.peek(30_000_000, LOOKAHEAD_NS));
  latch.push(frame(1, 30_000_000), true);
  let taken = latch.take(30_000_000, LOOKAHEAD_NS).expect("due");
  assert!(taken.late, "the gate did not see it");
  // A peek that saw it: not late.
  latch.push(frame(2, 60_000_000), true);
  assert!(latch.peek(60_000_000, LOOKAHEAD_NS));
  assert!(!latch.take(60_000_000, LOOKAHEAD_NS).expect("due").late);
  // A take for a different deadline than the peek's is not judged by it.
  latch.push(frame(3, 90_000_000), true);
  assert!(!latch.peek(80_000_000, 0));
  assert!(!latch.take(90_000_000, 0).expect("due").late);
  latch.close();
  assert_eq!(latch.push(frame(4, 0), true), Pushed::Closed);
  assert!(latch.take(1_000_000_000, 0).is_none());
}

#[test]
fn the_stepped_take_waits_for_the_frame_that_is_due_and_the_blocking_push_for_room() {
  use std::sync::Arc;
  use std::thread;
  use std::time::Duration;
  let shared = Arc::new(YuvLatchShared::new());
  shared.set_playing(true);
  // The producer pushes frames 40 ms apart, blocking when the latch is
  // full; it can only get past the cap once the consumer takes.
  let producer = {
    let shared = shared.clone();
    thread::spawn(move || {
      for k in 0..10 {
        shared.push(frame(k, k * 40_000_000), true);
      }
      shared.end();
    })
  };
  // The consumer's first take, for the frame at 0 ms, waits until a frame
  // due after it is queued (the producer has it at once) and takes exactly
  // the due frame.
  let taken = shared.take(0, 0, true).expect("the frame at 0 is due");
  assert_eq!(taken.frame.pts_us, 0);
  // 200 ms: the producer must push five more frames past the cap to reach
  // the one due after it; the take waits through the producer's blocking.
  let taken = shared.take(200_000_000, 0, true).expect("the frame at 200 ms");
  assert_eq!(taken.frame.pts_us, 5);
  assert_eq!(taken.skipped, 4, "the frames between the two takes were skipped, none shown");
  // Past the end: the take stops waiting once the stream has ended and
  // hands back the last frame.
  let taken = shared.take(10_000_000_000, 0, true).expect("the last frame");
  assert_eq!(taken.frame.pts_us, 9);
  assert!(shared.take(20_000_000_000, 0, true).is_none(), "nothing more after the end");
  producer.join().expect("producer");
  // A paused producer: the wait ends at once, with nothing.
  let idle = YuvLatchShared::new();
  idle.set_playing(false);
  let started = std::time::Instant::now();
  assert!(idle.take(0, 0, true).is_none());
  assert!(started.elapsed() < Duration::from_millis(100), "a paused latch never holds the capture");
}
