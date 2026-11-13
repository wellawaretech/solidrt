use crate::display::Rect;
use crate::window::{create_image_cursor, normalize_safe_area, resize_via_limits_enabled, start_units, window_units};

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

/// Logical window coordinates (macOS, Wayland): the display scale equals
/// the pixel density, so the logical size passes through.
#[test]
fn window_units_where_coordinates_are_logical_keep_the_size() {
  const SCALE: f32 = 2.0;
  const DENSITY: f32 = 2.0;
  assert_eq!(window_units((900, 600), SCALE, DENSITY), (900, 600));
}

/// Physical window coordinates (X11, Windows): density 1 at scale 2, so
/// the logical size doubles.
#[test]
fn window_units_where_coordinates_are_physical_scale_the_size() {
  const SCALE: f32 = 2.0;
  const DENSITY: f32 = 1.0;
  assert_eq!(window_units((900, 600), SCALE, DENSITY), (1800, 1200));
}

/// No density (or no scale) means a factor of 1.
#[test]
fn window_units_without_a_density_keep_the_size() {
  assert_eq!(window_units((900, 600), 2.0, 0.0), (900, 600));
  assert_eq!(window_units((900, 600), 0.0, 1.0), (900, 600));
}

/// A driver with physical window coordinates (X11): the content scale
/// multiplies the logical size.
#[test]
fn start_units_on_x11_scale_the_size() {
  const CONTENT_SCALE: f32 = 2.0;
  assert_eq!(start_units((900, 600), "x11", CONTENT_SCALE), (1800, 1200));
}

/// A fractional content scale rounds to whole units.
#[test]
fn start_units_round_to_whole_units() {
  const CONTENT_SCALE: f32 = 1.25;
  assert_eq!(start_units((901, 601), "windows", CONTENT_SCALE), (1126, 751));
}

/// A driver with logical window coordinates (Wayland): the size passes
/// through whatever the content scale.
#[test]
fn start_units_on_wayland_keep_the_size() {
  const CONTENT_SCALE: f32 = 2.0;
  assert_eq!(start_units((900, 600), "wayland", CONTENT_SCALE), (900, 600));
}

/// A driver outside the table keeps the size.
#[test]
fn start_units_on_an_unknown_driver_keep_the_size() {
  const CONTENT_SCALE: f32 = 2.0;
  assert_eq!(start_units((900, 600), "offscreen", CONTENT_SCALE), (900, 600));
  assert_eq!(start_units((900, 600), "", CONTENT_SCALE), (900, 600));
}

/// No content scale means a factor of 1.
#[test]
fn start_units_without_a_content_scale_keep_the_size() {
  assert_eq!(start_units((900, 600), "x11", 0.0), (900, 600));
  assert_eq!(start_units((900, 600), "x11", -1.0), (900, 600));
}

/// The resize-via-limits gate: any value but an empty string or "0" turns
/// it on; an unset variable leaves it off.
#[test]
fn resize_via_limits_reads_the_environment_value() {
  assert!(!resize_via_limits_enabled(None));
  assert!(!resize_via_limits_enabled(Some("")));
  assert!(!resize_via_limits_enabled(Some("0")));
  assert!(resize_via_limits_enabled(Some("1")));
  assert!(resize_via_limits_enabled(Some("true")));
}
