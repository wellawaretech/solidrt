use sdl3::event::Event;
use sdl3::sys::events::{
  SDL_Event, SDL_EventAction, SDL_PeepEvents, SDL_PumpEvents, SDL_EVENT_FIRST, SDL_EVENT_LAST,
};
use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::video::{SDL_GetWindowDisplayScale, SDL_GetWindowSafeArea};

/// Flush pending OS input into SDL's internal event queue.
///
/// Must be called from the main thread.
pub fn pump_events() {
  unsafe { SDL_PumpEvents() };
}

/// Safe area insets for the window in pixels (top, right, bottom, left).
///
/// Must be called from the main thread.
pub fn window_safe_area(window: &sdl3::video::Window) -> (i32, i32, i32, i32) {
  let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
  unsafe { SDL_GetWindowSafeArea(window.raw(), &mut rect) };
  let (full_w, full_h) = window.size_in_pixels();
  (
    rect.y,
    full_w as i32 - (rect.x + rect.w),
    full_h as i32 - (rect.y + rect.h),
    rect.x,
  )
}

/// Display scale factor (logical -> physical pixels) for the window.
///
/// Must be called from the main thread.
pub fn window_display_scale(window: &sdl3::video::Window) -> f32 {
  unsafe { SDL_GetWindowDisplayScale(window.raw()) }
}

/// Remove and return the next event from SDL's queue, or `None` if empty.
///
/// Thread-safe: `SDL_PeepEvents` does not pump and may be called from any thread.
pub fn poll_event() -> Option<Event> {
  let mut raw = SDL_Event::default();
  let n = unsafe {
    SDL_PeepEvents(
      &mut raw,
      1,
      SDL_EventAction::GETEVENT,
      SDL_EVENT_FIRST.0,
      SDL_EVENT_LAST.0,
    )
  };
  if n == 1 {
    Some(Event::from_ll(raw))
  } else {
    None
  }
}
