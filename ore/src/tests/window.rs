use crate::display::Rect;
use crate::window::{create_image_cursor, normalize_safe_area};

/// A desktop window: SDL_GetWindowSize is already logical, so the pixel
/// size over the scale equals it and the rect passes through.
#[test]
fn safe_area_on_desktop_stays_in_points() {
  const SCALE: f32 = 2.0;
  let rect = Rect { x: 0, y: 24, width: 800, height: 576 };
  let normalized = normalize_safe_area(rect, (800, 600), (1600, 1200), SCALE);
  assert_eq!(normalized, rect);
}

/// An Android window: SDL_GetWindowSize is the physical surface, so the
/// rect comes back in pixels and is divided by the scale.
#[test]
fn safe_area_on_android_is_divided_by_the_scale() {
  const SCALE: f32 = 2.0;
  let rect = Rect { x: 0, y: 48, width: 1600, height: 1152 };
  let normalized = normalize_safe_area(rect, (1600, 1200), (1600, 1200), SCALE);
  assert_eq!(normalized, Rect { x: 0, y: 24, width: 800, height: 576 });
}

/// A window with no size yet (or no scale) leaves the rect as SDL gave it.
#[test]
fn safe_area_without_a_size_is_left_alone() {
  let rect = Rect { x: 1, y: 2, width: 3, height: 4 };
  assert_eq!(normalize_safe_area(rect, (0, 0), (0, 0), 1.0), rect);
  assert_eq!(normalize_safe_area(rect, (10, 10), (10, 10), 0.0), rect);
}

#[test]
fn image_cursor_without_frames_is_an_error() {
  let error = create_image_cursor(&[], 0, 0).err();
  assert_eq!(error.as_deref(), Some("cursor has no frames"));
}
