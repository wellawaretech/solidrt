use sdl3::event::Event;
use sdl3::sys::events::{SDL_Event, SDL_PollEvent};
use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::video::{SDL_GetWindowDisplayScale, SDL_GetWindowSafeArea, SDL_Window};

pub fn drain_events(mut f: impl FnMut(Event)) {
  let mut raw = SDL_Event::default();
  while unsafe { SDL_PollEvent(&mut raw) } {
    f(Event::from_ll(raw));
  }
}

pub fn window_safe_area(window: *mut SDL_Window) -> SDL_Rect {
  let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
  unsafe { SDL_GetWindowSafeArea(window, &mut rect) };
  rect
}

pub fn window_display_scale(window: *mut SDL_Window) -> f32 {
  unsafe { SDL_GetWindowDisplayScale(window) }
}
