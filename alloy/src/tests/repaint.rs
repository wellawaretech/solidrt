//! The window's partial-repaint bookkeeping (raster::DamageTracker): a
//! delta folded in between frames reaches the next frame's own damage and
//! through the ring every aged back buffer, and a known buffer age routes
//! a frame the way an EGL one does, on a single-sample window through the
//! rig and on a multisampled one through the patch layer. Driven through
//! `known_age`, which skips the EGL buffer-age probe, and
//! `assume_window_patches`, which stands in for the damage region's; the
//! damage region's declarations through `fake_surface`, a window surface
//! with no EGL behind it.

use crate::raster::{
  overlay_repair, patch_layer_fit, stepped_patch, window_patch_refusal, DamageRect, DamageTracker, FakeSurface,
  LayerFit, PresentDamage, WindowBuffer, WindowRoute,
};
use crate::renderer::ISize;
use std::cell::RefCell;
use std::rc::Rc;

fn window() -> ISize {
  ISize::new(400, 300)
}

// The old overlay rectangle, and a small tree change elsewhere.
const OVERLAY: DamageRect = DamageRect { x: 300, y: 10, width: 80, height: 40 };
const WRITE: DamageRect = DamageRect { x: 20, y: 200, width: 10, height: 10 };

// A swapchain two buffers deep: every back buffer holds the frame before
// last.
const AGE_TWO: i32 = 2;

// One frame as frame.rs runs it on a `buffer` window: `damage` folds in
// with the frame, the frame takes everything pending, is routed with
// `known_age` and presents.
fn frame_on(tracker: &mut DamageTracker, buffer: WindowBuffer, damage: PresentDamage, known_age: i32) -> WindowRoute {
  tracker.fold(damage);
  let own = tracker.take();
  let route = tracker.route(own, window(), buffer, false, Some(known_age));
  tracker.presented(own);
  route
}

// One frame on a single-sample window.
fn frame(tracker: &mut DamageTracker, damage: PresentDamage, known_age: i32) -> WindowRoute {
  frame_on(tracker, WindowBuffer::SingleSample, damage, known_age)
}

fn contains(outer: DamageRect, inner: DamageRect) -> bool {
  outer.x <= inner.x
    && outer.y <= inner.y
    && outer.x + outer.width >= inner.x + inner.width
    && outer.y + outer.height >= inner.y + inner.height
}

// A tracker past its first frame, which redraws whole (no size recorded).
fn started(known_age: i32) -> DamageTracker {
  let mut tracker = DamageTracker::new();
  assert!(matches!(frame(&mut tracker, PresentDamage::Full, known_age), WindowRoute::Rig(None)));
  tracker
}

#[test]
fn a_folded_old_overlay_rect_reaches_both_aged_buffers() {
  let mut tracker = started(AGE_TWO);
  frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO);
  // The overlay leaves between frames: its old rectangle folds in.
  tracker.fold(PresentDamage::Rect(OVERLAY));
  // The next frame draws over the buffer from two frames ago, and the
  // frame after it over the one the first drew: both still show it.
  for n in 0..2 {
    match frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO) {
      WindowRoute::Rig(Some(patch)) => assert!(contains(patch, OVERLAY), "frame {n}: {patch:?}"),
      _ => panic!("frame {n}: expected a patch"),
    }
  }
}

// The overlay's new places: moved by its own width, and grown by it.
const OVERLAY_MOVED: DamageRect = DamageRect { x: 220, ..OVERLAY };
const OVERLAY_GROWN: DamageRect = DamageRect { width: 160, ..OVERLAY };

#[test]
fn an_overlay_that_leaves_its_rectangle_repairs_it() {
  let old = Some(PresentDamage::Rect(OVERLAY));
  assert_eq!(overlay_repair(Some(OVERLAY), None), old, "removed");
  assert_eq!(overlay_repair(Some(OVERLAY), Some(OVERLAY_MOVED)), old, "moved");
  assert_eq!(overlay_repair(Some(OVERLAY), Some(OVERLAY_GROWN)), old, "resized");
}

#[test]
fn an_overlay_that_keeps_its_rectangle_or_arrives_repairs_nothing() {
  // A new declaration over the same rectangle changed only its content:
  // the frame's own damage holds the rectangle.
  assert_eq!(overlay_repair(Some(OVERLAY), Some(OVERLAY)), None, "content only");
  assert_eq!(overlay_repair(None, Some(OVERLAY)), None, "first install");
}

#[test]
fn a_moved_overlay_repairs_both_aged_buffers() {
  let mut tracker = started(AGE_TWO);
  frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO);
  // The overlay moves between frames: what set_overlay folds.
  tracker.fold(overlay_repair(Some(OVERLAY), Some(OVERLAY_MOVED)).expect("a moved overlay repairs"));
  for n in 0..2 {
    match frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO) {
      WindowRoute::Rig(Some(patch)) => assert!(contains(patch, OVERLAY), "frame {n}: {patch:?}"),
      _ => panic!("frame {n}: expected a patch"),
    }
  }
}

#[test]
fn a_folded_full_damage_redraws_every_aged_buffer_whole() {
  let mut tracker = started(AGE_TWO);
  frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO);
  // A window shader is cleared between frames.
  tracker.fold(PresentDamage::Full);
  for n in 0..2 {
    assert!(matches!(frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO), WindowRoute::Rig(None)), "frame {n}");
  }
  // Both buffers hold a whole frame again: the next one patches.
  assert!(matches!(frame(&mut tracker, PresentDamage::Rect(WRITE), AGE_TWO), WindowRoute::Rig(Some(_))));
}

#[test]
fn a_known_age_routes_like_a_buffer_age() {
  // Age 1: the buffer holds the last frame, the patch is the frame's own.
  let mut tracker = started(1);
  match frame(&mut tracker, PresentDamage::Rect(WRITE), 1) {
    WindowRoute::Rig(Some(patch)) => assert_eq!(patch, WRITE),
    _ => panic!("expected the frame's own damage"),
  }
  // Age 0: the buffer holds nothing known.
  assert!(matches!(frame(&mut tracker, PresentDamage::Rect(WRITE), 0), WindowRoute::Rig(None)));
  // An age deeper than the ring holds.
  let mut fresh = started(1);
  assert!(matches!(frame(&mut fresh, PresentDamage::Rect(WRITE), AGE_TWO + 1), WindowRoute::Rig(None)));
  // A size change.
  tracker.fold(PresentDamage::Rect(WRITE));
  let own = tracker.take();
  let single = WindowBuffer::SingleSample;
  assert!(matches!(tracker.route(own, ISize::new(500, 300), single, false, Some(1)), WindowRoute::Rig(None)));
  // A patch-barred frame, at the size the ring now holds.
  assert!(matches!(tracker.route(own, ISize::new(500, 300), single, true, Some(1)), WindowRoute::Rig(None)));
  // The same frame unbarred patches.
  assert!(matches!(
    tracker.route(own, ISize::new(500, 300), single, false, Some(1)),
    WindowRoute::Rig(Some(patch)) if patch == WRITE
  ));
}

// The patch layer's size step (raster::repaint's PATCH_LAYER_STEP), pixels.
const PATCH_STEP: i32 = 64;

// A multisampled window, past its first frame (drawn whole), that can
// declare its damage region when `patches`.
fn started_multisampled(patches: bool, known_age: i32) -> DamageTracker {
  let mut tracker = DamageTracker::new();
  tracker.assume_window_patches(patches);
  assert_eq!(frame_on(&mut tracker, WindowBuffer::Multisampled, PresentDamage::Full, known_age), WindowRoute::FastPath);
  tracker
}

fn multisampled(tracker: &mut DamageTracker, damage: DamageRect, known_age: i32) -> WindowRoute {
  frame_on(tracker, WindowBuffer::Multisampled, PresentDamage::Rect(damage), known_age)
}

// Three small writes far apart, one per frame.
const FIRST: DamageRect = WRITE;
const SECOND: DamageRect = DamageRect { x: 100, y: 40, width: 10, height: 10 };
const THIRD: DamageRect = DamageRect { x: 200, y: 20, width: 10, height: 10 };

#[test]
fn a_multisampled_window_patches_through_the_layer_at_every_age() {
  let mut tracker = started_multisampled(true, 1);
  // Age 1: the frame's own damage, grown to one step each way.
  assert_eq!(
    multisampled(&mut tracker, FIRST, 1),
    WindowRoute::PatchLayer(DamageRect { x: 20, y: 200, width: 64, height: 64 })
  );
  // Age 2: with the last frame's, from (20, 40) to (110, 210), grown to
  // whole steps.
  assert_eq!(
    multisampled(&mut tracker, SECOND, 2),
    WindowRoute::PatchLayer(DamageRect { x: 20, y: 40, width: 128, height: 192 })
  );
  // Age 3: with the last two frames', from (20, 20) to (210, 210).
  match multisampled(&mut tracker, THIRD, 3) {
    WindowRoute::PatchLayer(patch) => {
      assert_eq!(patch, DamageRect { x: 20, y: 20, width: 192, height: 192 });
      for rect in [FIRST, SECOND, THIRD] {
        assert!(contains(patch, rect), "{rect:?} outside {patch:?}");
      }
    }
    route => panic!("expected a patch-layer patch, got {route:?}"),
  }
}

#[test]
fn a_multisampled_window_that_cannot_declare_draws_whole() {
  // No damage region offered, or switched off: the tracker is told so.
  let mut tracker = started_multisampled(false, 1);
  for age in 1..=3 {
    assert_eq!(multisampled(&mut tracker, FIRST, age), WindowRoute::FastPath, "age {age}");
  }
}

#[test]
fn the_kill_switch_and_a_missing_extension_refuse_window_patches() {
  assert_eq!(window_patch_refusal(None, true), None);
  assert!(window_patch_refusal(Some("off"), true).is_some());
  assert!(window_patch_refusal(Some(" off "), true).is_some());
  assert!(window_patch_refusal(None, false).is_some());
  assert!(window_patch_refusal(Some("off"), false).is_some());
  // Any other value is ignored, with a warning.
  assert_eq!(window_patch_refusal(Some("on"), true), None);
}

#[test]
fn a_multisampled_window_draws_whole_where_anything_is_uncertain() {
  let ms = WindowBuffer::Multisampled;
  // Full damage.
  let mut tracker = started_multisampled(true, 1);
  assert_eq!(frame_on(&mut tracker, ms, PresentDamage::Full, 1), WindowRoute::FastPath);
  // Age 0: the buffer holds nothing known.
  assert_eq!(multisampled(&mut tracker, FIRST, 0), WindowRoute::FastPath);
  // An age deeper than the ring holds.
  let mut fresh = started_multisampled(true, 1);
  assert_eq!(multisampled(&mut fresh, FIRST, AGE_TWO + 1), WindowRoute::FastPath);
  // A size change.
  let own = PresentDamage::Rect(FIRST);
  assert_eq!(tracker.route(own, ISize::new(500, 300), ms, false, Some(1)), WindowRoute::FastPath);
  // A patch-barred frame (a window shader, a window tap), at the size the
  // ring now holds.
  assert_eq!(tracker.route(own, ISize::new(500, 300), ms, true, Some(1)), WindowRoute::FastPath);
  // The same frame unbarred patches.
  assert!(matches!(tracker.route(own, ISize::new(500, 300), ms, false, Some(1)), WindowRoute::PatchLayer(_)));
}

// A write just under half the 400 x 300 window, which its growth to whole
// steps takes over half (256 x 256); and one that stays under (192 x 192).
const UNDER_UNTIL_STEPPED: DamageRect = DamageRect { x: 0, y: 0, width: 250, height: 230 };
const UNDER: DamageRect = DamageRect { x: 0, y: 0, width: 190, height: 190 };

#[test]
fn a_patch_over_half_the_window_after_its_steps_draws_whole() {
  let mut tracker = started_multisampled(true, 1);
  assert_eq!(multisampled(&mut tracker, UNDER_UNTIL_STEPPED, 1), WindowRoute::FastPath);
  assert_eq!(
    multisampled(&mut tracker, UNDER, 1),
    WindowRoute::PatchLayer(DamageRect { x: 0, y: 0, width: 192, height: 192 })
  );
}

#[test]
fn an_empty_damage_is_an_empty_patch_layer_patch() {
  let mut tracker = started_multisampled(true, 1);
  assert_eq!(
    frame_on(&mut tracker, WindowBuffer::Multisampled, PresentDamage::None, 1),
    WindowRoute::PatchLayer(DamageRect { x: 0, y: 0, width: 0, height: 0 })
  );
}

#[test]
fn the_headless_layer_mode_patches_through_the_layer_and_draws_whole_through_the_rig() {
  // The test switch's layer mode needs no damage region: nothing is
  // presented.
  let mut tracker = DamageTracker::new();
  let layer = WindowBuffer::HeadlessLayer;
  assert_eq!(frame_on(&mut tracker, layer, PresentDamage::Full, 1), WindowRoute::Rig(None));
  assert_eq!(
    frame_on(&mut tracker, layer, PresentDamage::Rect(FIRST), 1),
    WindowRoute::PatchLayer(DamageRect { x: 20, y: 200, width: 64, height: 64 })
  );
  assert_eq!(frame_on(&mut tracker, layer, PresentDamage::Rect(UNDER_UNTIL_STEPPED), 1), WindowRoute::Rig(None));
}

#[test]
fn a_stepped_patch_holds_the_patch_inside_the_window() {
  let (width, height) = (window().width as i32, window().height as i32);
  let step = PATCH_STEP;
  for x in (0..width).step_by(37) {
    for y in (0..height).step_by(29) {
      for (w, h) in [(1, 1), (10, 70), (64, 64), (65, 3), (width - x, height - y)] {
        let p = DamageRect { x, y, width: w.min(width - x), height: h.min(height - y) };
        let s = stepped_patch(p, width, height);
        assert!(contains(s, p), "{s:?} does not hold {p:?}");
        assert!(s.x >= 0 && s.y >= 0 && s.x + s.width <= width && s.y + s.height <= height, "{s:?} leaves the window");
        assert!(s.width % step == 0 || s.width == width, "{s:?} width not a whole step");
        assert!(s.height % step == 0 || s.height == height, "{s:?} height not a whole step");
        assert!(s.width < p.width + step && s.height < p.height + step, "{s:?} grew past one step for {p:?}");
      }
    }
  }
}

#[test]
fn a_stepped_patch_moves_in_at_the_right_and_bottom_edges() {
  let p = DamageRect { x: 390, y: 290, width: 5, height: 5 };
  assert_eq!(stepped_patch(p, 400, 300), DamageRect { x: 336, y: 236, width: 64, height: 64 });
  // A whole step stays as it is.
  let exact = DamageRect { x: 64, y: 0, width: 64, height: 64 };
  assert_eq!(stepped_patch(exact, 400, 300), exact);
}

#[test]
fn a_stepped_patch_in_a_window_smaller_than_a_step_is_the_window() {
  let p = DamageRect { x: 10, y: 10, width: 5, height: 5 };
  assert_eq!(stepped_patch(p, 50, 40), DamageRect { x: 0, y: 0, width: 50, height: 40 });
}

// A swapchain three buffers deep, in steady state: every back buffer holds
// the frame three presents ago.
const DEPTH: i32 = 3;

// A multisampled window on a fake surface, past its warm-up: the first
// frame draws whole (no size recorded), the next whole too (the ring is
// shorter than the age), and from then on a small write patches and
// declares its region.
fn on_fake_surface() -> (DamageTracker, Rc<RefCell<FakeSurface>>) {
  let mut tracker = DamageTracker::new();
  let fake = tracker.fake_surface();
  fake.borrow_mut().age = DEPTH;
  for _ in 0..DEPTH - 1 {
    assert_eq!(declared_frame(&mut tracker, &fake, true), (WindowRoute::FastPath, false), "warm-up");
  }
  assert!(matches!(declared_frame(&mut tracker, &fake, true), (WindowRoute::PatchLayer(_), true)), "warmed up");
  (tracker, fake)
}

// One small write's frame on the fake surface, as frame.rs runs it: the
// route, then `presented` or, when the frame did not reach a swap,
// `not_presented`. Returns the route and whether it declared a region.
fn declared_frame(tracker: &mut DamageTracker, fake: &Rc<RefCell<FakeSurface>>, present: bool) -> (WindowRoute, bool) {
  tracker.fold(PresentDamage::Rect(FIRST));
  let own = tracker.take();
  let before = fake.borrow().declared.len();
  let route = tracker.route(own, window(), WindowBuffer::Multisampled, false, None);
  match present {
    true => tracker.presented(own),
    false => tracker.not_presented(own),
  }
  (route, fake.borrow().declared.len() > before)
}

// After the present that may hold undefined pixels (the next one after a
// frame left a region declared), every back buffer that holds it or an
// older one - the next DEPTH frames - draws whole and declares nothing;
// the frame after them, on a buffer drawn whole since, patches again.
fn whole_until_the_chain_is_redrawn(tracker: &mut DamageTracker, fake: &Rc<RefCell<FakeSurface>>) {
  for n in 0..DEPTH {
    assert_eq!(
      declared_frame(tracker, fake, true),
      (WindowRoute::FastPath, false),
      "frame {n} after the untrusted one"
    );
  }
  assert!(matches!(declared_frame(tracker, fake, true), (WindowRoute::PatchLayer(_), true)), "chain redrawn");
}

#[test]
fn a_declared_frame_that_is_not_presented_declares_nothing_until_the_chain_is_redrawn() {
  let (mut tracker, fake) = on_fake_surface();
  // The frame declares its patch and never reaches a swap: the region may
  // still be set on the back buffer.
  assert!(matches!(declared_frame(&mut tracker, &fake, false), (WindowRoute::PatchLayer(_), true)));
  // The next frame draws whole into that buffer, declaring nothing; the
  // buffer it presents is not trusted.
  assert_eq!(declared_frame(&mut tracker, &fake, true), (WindowRoute::FastPath, false), "the frame after");
  whole_until_the_chain_is_redrawn(&mut tracker, &fake);
}

#[test]
fn frames_that_keep_failing_after_a_declaration_stay_whole_until_one_presents() {
  let (mut tracker, fake) = on_fake_surface();
  assert!(matches!(declared_frame(&mut tracker, &fake, false), (WindowRoute::PatchLayer(_), true)));
  for n in 0..DEPTH {
    assert_eq!(declared_frame(&mut tracker, &fake, false), (WindowRoute::FastPath, false), "failed frame {n}");
  }
  assert_eq!(declared_frame(&mut tracker, &fake, true), (WindowRoute::FastPath, false), "the first present");
  whole_until_the_chain_is_redrawn(&mut tracker, &fake);
}

#[test]
fn a_declared_frame_that_is_presented_leaves_nothing_declared() {
  let (mut tracker, fake) = on_fake_surface();
  // The swap is the frame boundary: every frame declares and patches.
  for n in 0..DEPTH + 1 {
    assert!(matches!(declared_frame(&mut tracker, &fake, true), (WindowRoute::PatchLayer(_), true)), "frame {n}");
  }
  assert!(tracker.window_patches());
}

#[test]
fn a_rebind_retry_after_a_declaration_distrusts_the_buffer_it_presents() {
  let (mut tracker, fake) = on_fake_surface();
  // The frame declares, its swap fails, the surface is rebound (which may
  // have kept the surface and the region), and the retry draws whole and
  // presents.
  tracker.fold(PresentDamage::Rect(FIRST));
  let own = tracker.take();
  assert!(matches!(tracker.route(own, window(), WindowBuffer::Multisampled, false, None), WindowRoute::PatchLayer(_)));
  tracker.invalidated();
  tracker.presented(PresentDamage::Full);
  whole_until_the_chain_is_redrawn(&mut tracker, &fake);
}

#[test]
fn a_surface_replaced_with_no_declaration_outstanding_holds_nothing_against_the_chain() {
  let (mut tracker, fake) = on_fake_surface();
  // A frame that declares nothing (barred: a window tap), its swap fails,
  // the surface is replaced and the retry presents whole.
  tracker.fold(PresentDamage::Rect(FIRST));
  let own = tracker.take();
  let before = fake.borrow().declared.len();
  assert_eq!(tracker.route(own, window(), WindowBuffer::Multisampled, true, None), WindowRoute::FastPath);
  assert_eq!(fake.borrow().declared.len(), before, "a barred frame declares nothing");
  tracker.invalidated();
  tracker.presented(PresentDamage::Full);
  // The ring refills as after any replace: whole while the age is past it,
  // then patches, with no extra lap.
  for n in 0..DEPTH - 1 {
    assert_eq!(declared_frame(&mut tracker, &fake, true), (WindowRoute::FastPath, false), "ring refill {n}");
  }
  assert!(matches!(declared_frame(&mut tracker, &fake, true), (WindowRoute::PatchLayer(_), true)));
}

#[test]
fn a_refused_declaration_switches_window_patches_off() {
  let (mut tracker, fake) = on_fake_surface();
  let tries = fake.borrow().declared.len();
  fake.borrow_mut().refuse = true;
  assert_eq!(declared_frame(&mut tracker, &fake, true), (WindowRoute::FastPath, true), "the refused frame");
  assert!(!tracker.window_patches());
  // Off for this tracker's context: nothing is declared again, even where
  // the driver would take it.
  fake.borrow_mut().refuse = false;
  for n in 0..DEPTH + 1 {
    assert_eq!(declared_frame(&mut tracker, &fake, true), (WindowRoute::FastPath, false), "frame {n} after");
  }
  assert_eq!(fake.borrow().declared.len(), tries + 1);
}

#[test]
fn a_refusal_while_our_own_declaration_is_outstanding_keeps_window_patches_on() {
  let (mut tracker, fake) = on_fake_surface();
  // Two routes with no outcome between them: the second declaration is
  // refused because the first is still set, which says nothing of the
  // driver.
  tracker.fold(PresentDamage::Rect(FIRST));
  let own = tracker.take();
  assert!(matches!(tracker.route(own, window(), WindowBuffer::Multisampled, false, None), WindowRoute::PatchLayer(_)));
  fake.borrow_mut().refuse = true;
  assert_eq!(tracker.route(own, window(), WindowBuffer::Multisampled, false, None), WindowRoute::FastPath);
  assert!(tracker.window_patches());
}

// Patch sizes, pixels: one step, seven steps, and four steps across.
const STEP_PATCH: (u32, u32) = (64, 64);
const WIDE_PATCH: (u32, u32) = (448, 448);
const FOUR_STEPS: (u32, u32) = (256, 64);

#[test]
fn the_patch_layer_grows_to_hold_every_patch_it_meets() {
  assert_eq!(patch_layer_fit(None, STEP_PATCH, 0), (LayerFit::Allocate(64, 64), 0));
  // Grown to hold the old and the new: a patch wide one way, then the
  // other, ends in a layer that holds both.
  assert_eq!(patch_layer_fit(Some(FOUR_STEPS), (64, 256), 0), (LayerFit::Allocate(256, 256), 0));
  // Within the bound it stays.
  assert_eq!(patch_layer_fit(Some((128, 128)), STEP_PATCH, 0), (LayerFit::Keep, 0));
}

#[test]
fn a_patch_alternating_between_two_sizes_reallocates_the_layer_once() {
  let (mut layer, mut oversized) = (None, 0);
  let mut allocations = 0;
  for frame in 0..1000 {
    let need = if frame % 2 == 0 { STEP_PATCH } else { WIDE_PATCH };
    let (fit, next) = patch_layer_fit(layer, need, oversized);
    oversized = next;
    if let LayerFit::Allocate(w, h) = fit {
      allocations += 1;
      layer = Some((w, h));
    }
  }
  // The first frame's, then the large patch's growth.
  assert_eq!(allocations, 2);
  assert_eq!(layer, Some(WIDE_PATCH));
}

#[test]
fn a_layer_left_large_by_one_patch_shrinks_after_a_run_of_small_ones() {
  let mut oversized = 0;
  let mut frames = 0;
  loop {
    frames += 1;
    let (fit, next) = patch_layer_fit(Some(WIDE_PATCH), STEP_PATCH, oversized);
    oversized = next;
    if fit != LayerFit::Keep {
      assert_eq!(fit, LayerFit::Allocate(64, 64));
      break;
    }
    assert!(frames < 1000, "the layer never shrinks");
  }
  // Not at once: a run of frames.
  assert!(frames > 1, "shrank after {frames} frames");
}
