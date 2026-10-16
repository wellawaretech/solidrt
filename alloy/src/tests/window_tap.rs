//! The window tap's size and rect arithmetic (see raster/window_tap.rs):
//! the output size from what was asked and the source, and a cut-out
//! against the surface. No GL: the readback itself needs a context.

use crate::gl::TapRect;
use crate::raster::window_tap::{clamp_rect, resolve_size};
use crate::raster::TapSize;

fn size(width: Option<u32>, height: Option<u32>) -> TapSize {
  TapSize { width, height }
}

#[test]
fn nothing_asked_is_the_source_rounded_even() {
  assert_eq!(resolve_size(size(None, None), (1280, 720)), Some((1280, 720)));
  assert_eq!(resolve_size(size(None, None), (1279, 721)), Some((1280, 722)));
}

#[test]
fn one_dimension_keeps_the_aspect() {
  assert_eq!(resolve_size(size(None, Some(720)), (1920, 1080)), Some((1280, 720)));
  assert_eq!(resolve_size(size(Some(640), None), (1920, 1080)), Some((640, 360)));
  // 540 of a 4:3 source is 405 wide: rounded up to even.
  assert_eq!(resolve_size(size(Some(540), None), (640, 480)), Some((540, 406)));
}

#[test]
fn both_dimensions_are_as_asked_and_at_least_two() {
  assert_eq!(resolve_size(size(Some(300), Some(200)), (1920, 1080)), Some((300, 200)));
  assert_eq!(resolve_size(size(Some(1), Some(0)), (1920, 1080)), Some((2, 2)));
}

#[test]
fn an_empty_source_has_no_size() {
  assert_eq!(resolve_size(size(None, Some(720)), (0, 0)), None);
  assert_eq!(resolve_size(size(Some(640), Some(480)), (640, 0)), None);
}

#[test]
fn a_rect_is_cut_to_the_surface() {
  let rect = TapRect { x: 100, y: 50, width: 300, height: 300 };
  assert_eq!(clamp_rect(rect, 1000, 1000), Some(rect));
  assert_eq!(clamp_rect(rect, 250, 1000), Some(TapRect { x: 100, y: 50, width: 150, height: 300 }));
  assert_eq!(clamp_rect(rect, 1000, 200), Some(TapRect { x: 100, y: 50, width: 300, height: 150 }));
  assert_eq!(clamp_rect(rect, 100, 1000), None);
  assert_eq!(clamp_rect(rect, 1000, 50), None);
  assert_eq!(clamp_rect(TapRect { x: u32::MAX, y: 0, width: 10, height: 10 }, 1000, 1000), None);
}
