use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::video::{SDL_GetWindowDisplayScale, SDL_GetWindowSafeArea};

pub fn window_safe_area(window: &sdl3::video::Window) -> SDL_Rect {
  let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
  unsafe { SDL_GetWindowSafeArea(window.raw(), &mut rect) };
  rect
}

pub fn window_display_scale(window: &sdl3::video::Window) -> f32 {
  unsafe { SDL_GetWindowDisplayScale(window.raw()) }
}
