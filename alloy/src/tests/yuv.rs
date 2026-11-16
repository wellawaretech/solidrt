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
use std::collections::HashSet;

fn frame(pts_us: i64, due_ns: i64) -> LatchedFrame {
  LatchedFrame { due_ns, pts_us, data: vec![pts_us as u8] }
}

// A latch something samples: a new one is unshown and parks its frames
// (see `YuvLatch::set_shown`).
fn shown_latch() -> YuvLatch {
  let mut latch = YuvLatch::new();
  assert!(latch.set_shown(true).is_none(), "nothing parked yet");
  latch
}

// A 50 Hz grid: 20 ms periods, a 10 ms lookahead.
const PERIOD_NS: i64 = 20_000_000;
const LOOKAHEAD_NS: i64 = PERIOD_NS / 2;

#[test]
fn the_take_hands_back_the_newest_due_frame_and_counts_the_skipped() {
  let mut latch = shown_latch();
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
  let mut latch = shown_latch();
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
  let mut latch = shown_latch();
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
  let mut latch = shown_latch();
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
  shared.set_shown(true);
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
  idle.set_shown(true);
  let started = std::time::Instant::now();
  assert!(idle.take(0, 0, true).is_none());
  assert!(started.elapsed() < Duration::from_millis(100), "a paused latch never holds the capture");
}

#[test]
fn an_unshown_latch_parks_its_newest_frame_and_becoming_shown_hands_it_back() {
  let mut latch = YuvLatch::new();
  latch.push(frame(1, 0), true);
  latch.push(frame(2, PERIOD_NS), true);
  // New, nothing samples it: the take pops on schedule (playback
  // advances) but hands nothing to the upload.
  assert!(latch.take(0, 0).is_none());
  assert_eq!(latch.shown_pts_us(), Some(1), "the position advances while unshown");
  assert!(latch.take(PERIOD_NS, 0).is_none());
  assert!(latch.is_empty(), "the queue drains, a live producer is never held up");
  // Becoming shown hands back the parked frame at once, for the upload
  // ahead of any observer; the one it replaced while parked is no skip,
  // nothing was meant to show it.
  let taken = latch.set_shown(true).expect("the parked frame");
  assert_eq!(taken.frame.pts_us, 2);
  assert_eq!(taken.skipped, 0);
  assert!(latch.set_shown(true).is_none(), "already shown: nothing to hand back");
  assert!(latch.take(2 * PERIOD_NS, 0).is_none(), "handed back once");
  // Shown, a take hands its frame straight to the upload.
  latch.push(frame(3, 3 * PERIOD_NS), true);
  assert_eq!(latch.take(3 * PERIOD_NS, 0).expect("due").frame.pts_us, 3);
  // Unshown with nothing parked: becoming shown again has nothing to give.
  assert!(latch.set_shown(false).is_none());
  assert!(latch.set_shown(true).is_none());
  // A readback takes the parked frame of an unshown latch; a close drops it.
  latch.set_shown(false);
  latch.push(frame(5, 6 * PERIOD_NS), true);
  assert!(latch.take(6 * PERIOD_NS, 0).is_none());
  assert_eq!(latch.take_held().expect("parked").frame.pts_us, 5);
  latch.push(frame(6, 7 * PERIOD_NS), true);
  assert!(latch.take(7 * PERIOD_NS, 0).is_none());
  latch.close();
  assert!(latch.take_held().is_none());
  assert!(latch.set_shown(true).is_none(), "a closed latch has nothing parked");
}

#[test]
fn an_unshown_latch_counts_no_skips() {
  let mut latch = YuvLatch::new();
  // Two frames due at one take: the older is superseded, a skip if shown.
  latch.push(frame(1, 0), true);
  latch.push(frame(2, 0), true);
  assert!(latch.take(0, 0).is_none());
  // A push against a full queue evicts, a skip too if shown.
  for k in 0..=LATCH_QUEUE_FRAMES as i64 {
    latch.push(frame(10 + k, (1 + k) * PERIOD_NS), true);
  }
  let taken = latch.set_shown(true).expect("the parked frame");
  assert_eq!(taken.frame.pts_us, 2);
  assert_eq!(taken.skipped, 0, "nothing was meant to show the superseded or the evicted frame");
  // Shown from the start, the same two kinds count.
  let mut shown = shown_latch();
  for k in 0..=LATCH_QUEUE_FRAMES as i64 {
    shown.push(frame(k, 0), true);
  }
  let taken = shown.take(0, 0).expect("due");
  assert_eq!(taken.skipped, LATCH_QUEUE_FRAMES, "one evicted and the superseded rest");
}

// --- The shown set on the ordered channel (see Context::sync_shown_video) ---

// What reached the raster channel, in order: a shown set, a pixel
// observer (here a display-list rasterize), or anything else. A stand-in
// raster thread refuses each command, which answers a blocked rpc.
fn recorded_commands(run: impl FnOnce(&crate::Context)) -> Vec<String> {
  use crate::raster::RasterCmd;
  let stats = std::sync::Arc::new(crate::raster::RasterStats::new());
  let (tx, rx) = std::sync::mpsc::channel::<RasterCmd>();
  let raster = std::thread::spawn(move || {
    let mut seen = Vec::new();
    for cmd in rx {
      seen.push(match &cmd {
        RasterCmd::ShowVideo { shown } => {
          let mut ids: Vec<u64> = shown.iter().copied().collect();
          ids.sort_unstable();
          format!("show {ids:?}")
        }
        RasterCmd::RasterizeDl { .. } => "observe".to_string(),
        RasterCmd::RenderTarget { .. } => "render".to_string(),
        RasterCmd::CopyTexture { .. } => "copy".to_string(),
        RasterCmd::Frame { tree_clean: true, .. } => "reused frame".to_string(),
        RasterCmd::Frame { .. } => "frame".to_string(),
        _ => "other".to_string(),
      });
      cmd.refuse("no raster thread in a lib test");
    }
    seen
  });
  {
    let ctx = crate::Context::new(crate::raster::RasterSender::new(tx, stats.clone()), stats, None);
    run(&ctx);
  }
  raster.join().expect("the stand-in raster thread")
}

fn observe(ctx: &crate::Context) {
  use crate::renderer::prelude::*;
  use crate::renderer::{DisplayListBuilder, Paint, Point, Rect, Size};
  let mut b = DisplayListBuilder::new(None);
  b.draw_rect(&Rect::new(Point::zero(), Size::new(1.0, 1.0)), &Paint::default());
  let dl = b.build().expect("the list builds");
  assert!(ctx.render_display_list_to_texture(&dl, 1, 1, true).is_err(), "refused by the stand-in");
}

#[test]
fn a_video_that_becomes_shown_is_announced_ahead_of_the_first_observer_of_it() {
  let seen = recorded_commands(|ctx| {
    ctx.insert_yuv_group_for_test(7);
    // Nothing samples it: a new latch is unshown already, nothing to say.
    ctx.note_shown_video(HashSet::new());
    observe(ctx);
    // A view sampling it mounts: the shown set goes out with the walk's
    // note, ahead of the walk's observers.
    ctx.note_shown_video([7].into_iter().collect());
    observe(ctx);
    observe(ctx);
    // A texture the tree already samples comes into being between walks:
    // the next observer's own command brings the set up to date first.
    ctx.note_shown_video([7, 9].into_iter().collect());
    ctx.insert_yuv_group_for_test(9);
    observe(ctx);
    // The view goes: unshown again.
    ctx.note_shown_video(HashSet::new());
  });
  assert_eq!(seen, ["observe", "show [7]", "observe", "observe", "show [7, 9]", "observe", "show []"]);
}

#[test]
fn a_target_binding_shows_a_video_and_unbinding_it_unshows_it() {
  let seen = recorded_commands(|ctx| {
    ctx.insert_yuv_group_for_test(7);
    observe(ctx);
    // A target samples the video, no tree node does: shown all the same,
    // ahead of the next observer (a manual target's render included).
    ctx.bind_source_for_test(40, "uVideo", Some(7));
    observe(ctx);
    observe(ctx);
    // The binding goes: unshown at the next observer.
    ctx.bind_source_for_test(40, "uVideo", None);
    observe(ctx);
  });
  assert_eq!(seen, ["observe", "show [7]", "observe", "observe", "show []", "observe"]);
}

#[test]
fn a_fire_and_forget_observer_after_a_binding_sees_the_video_shown() {
  use crate::raster::RasterCmd;
  let seen = recorded_commands(|ctx| {
    ctx.insert_yuv_group_for_test(7);
    // A render-once manual target binds the video and renders in the same
    // tick, no rpc and no frame between: the shown set goes out ahead of
    // the render, or the render samples the stale parked frame for good.
    ctx.bind_source_for_test(40, "uVideo", Some(7));
    ctx.send_for_test(RasterCmd::RenderTarget { id: 40, face: None, level: None });
    ctx.send_for_test(RasterCmd::CopyTexture { src: 40, dst: 41 });
    // Unbound: unshown ahead of the next copy.
    ctx.bind_source_for_test(40, "uVideo", None);
    ctx.send_for_test(RasterCmd::CopyTexture { src: 40, dst: 41 });
  });
  assert_eq!(seen, ["show [7]", "render", "copy", "show []", "copy"]);
}

#[test]
fn the_paint_walk_announces_a_newly_shown_video_ahead_of_its_own_observers() {
  use crate::renderer::prelude::*;
  use crate::renderer::DisplayListBuilder;
  use crate::rendertree::composite::paint_phase;
  use crate::rendertree::*;
  let seen = recorded_commands(|ctx| {
    ctx.insert_yuv_group_for_test(7);
    // root(1) > snapshot boundary(2) > texture(3) sampling the video: the
    // walk rasterizes the boundary (a pixel observer) while it walks, so
    // the shown set must be out before the walk's first observer, not
    // only before the frame.
    let mut tree = RenderTree::new();
    tree.create_node(1, View::default().with_layout());
    tree.create_node(2, View::default().with_layout());
    tree.create_node(3, Texture { texture_id: Some(7), ..Default::default() }.with_layout());
    tree.insert_node(1, 2, None).expect("insert");
    tree.insert_node(2, 3, None).expect("insert");
    tree.root = Some(1);
    tree.edit(2, |el| {
      el.repaint_boundary = BoundaryMode::Snapshot;
      Damage::None
    });
    for (id, side) in [(1, 100.0), (2, 100.0), (3, 50.0)] {
      let style = tree.node_mut(id).style_mut().expect("laid-out node");
      style.size = taffy::Size { width: taffy::prelude::length(side), height: taffy::prelude::length(side) };
    }
    let platform = PlatformContext::new(Vec::new());
    platform.set_window_size(100.0, 100.0);
    paint_phase(&mut DisplayListBuilder::new(None), &mut tree, &platform, ctx);
  });
  let first_observer = seen.iter().position(|c| c == "observe").expect("the walk rasterizes the boundary");
  let shown = seen.iter().position(|c| c == "show [7]").expect("the video is announced");
  assert!(shown < first_observer, "announced after the walk's first observer: {seen:?}");
}

#[test]
fn a_window_shader_write_on_the_reuse_path_announces_its_video_ahead_of_the_frame() {
  use crate::rendertree::frame::{Commit, FrameDriver};
  use crate::rendertree::*;
  let seen = recorded_commands(|ctx| {
    ctx.insert_yuv_group_for_test(7);
    let mut tree = RenderTree::new();
    tree.create_node(1, Element::with_layout(ElementKind::Window(Window::default()), Window::initial_style()));
    tree.root = Some(1);
    let platform = PlatformContext::new(Vec::new());
    platform.set_window_size(100.0, 100.0);
    let mut driver = FrameDriver::new();
    let at = std::time::Instant::now();
    // A first frame builds and retains the list; nothing samples the video.
    match driver.begin(&platform, true).expect("demanded").commit(&mut tree, &platform, ctx, at, 1) {
      Ok(Commit::Build(mut b)) => {
        b.layout(&mut tree, &platform, ctx);
        b.paint(&mut tree, &platform, ctx);
        b.finish(&tree, &platform, ctx, at, 1).expect("submitted");
      }
      _ => panic!("the first frame builds"),
    }
    // A window shader that samples the video is declared, nothing else
    // changes: Damage::Present bumps no revision, so the frame reuses the
    // list and no paint walk notes the shader's inputs.
    tree.edit(1, |el| match &mut el.kind {
      ElementKind::Window(w) => w.set_shader(Some(crate::gpu::WindowShader {
        program: 1,
        params: vec![],
        textures: vec![crate::gpu::TextureBinding::new("uVideo", 7)],
        vertex_count: 3,
        previous: false,
      })),
      _ => unreachable!(),
    });
    match driver.begin(&platform, true).expect("demanded").commit(&mut tree, &platform, ctx, at, 2) {
      Ok(Commit::Reused { .. }) => {}
      _ => panic!("a window-shader write reuses the list"),
    }
  });
  let frames: Vec<&String> = seen.iter().filter(|c| c.as_str() != "other").collect();
  assert_eq!(frames, ["frame", "show [7]", "reused frame"], "all commands: {seen:?}");
}

#[test]
fn the_stepped_take_of_an_unshown_latch_still_makes_room_for_a_blocked_producer() {
  use std::sync::Arc;
  use std::thread;
  // Twice the cap: the producer can only finish if takes that park (and
  // hand back nothing) still wake it.
  const FRAMES: i64 = 2 * LATCH_QUEUE_FRAMES as i64;
  let shared = Arc::new(YuvLatchShared::new());
  shared.set_playing(true);
  let producer = {
    let shared = shared.clone();
    thread::spawn(move || {
      for k in 0..FRAMES {
        shared.push(frame(k, k * PERIOD_NS), true);
      }
      shared.end();
    })
  };
  // Unshown: each take parks the due frame and hands back nothing, yet
  // waits until it is final, which past the cap needs the producer woken.
  for k in 0..FRAMES {
    assert!(shared.take(k * PERIOD_NS, 0, true).is_none(), "an unshown take uploads nothing");
    assert_eq!(shared.shown_pts_us(), Some(k), "the position advances while unshown");
  }
  producer.join().expect("the producer got past the cap");
  // Becoming shown hands back the newest parked frame.
  let taken = shared.set_shown(true).expect("the parked frame");
  assert_eq!(taken.frame.pts_us, FRAMES - 1);
}
