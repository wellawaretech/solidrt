use sdl3::sys::keyboard::SDL_GetModState;
use sdl3::sys::power::{SDL_GetPowerInfo, SDL_PowerState};
use sdl3::sys::rect::SDL_Rect;
use sdl3::sys::video::{SDL_GetWindowDisplayScale, SDL_GetWindowSafeArea};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
  OnBattery,
  Charging,
  Charged,
  NoBattery,
  Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct PowerInfo {
  pub state: PowerState,
  pub percent: Option<u8>,
}

pub fn get_power_info() -> PowerInfo {
  let mut seconds: std::ffi::c_int = -1; //ignore, state and percentage only
  let mut percent: std::ffi::c_int = -1;
  let state = unsafe { SDL_GetPowerInfo(&mut seconds, &mut percent) };
  PowerInfo {
    state: match state {
      SDL_PowerState::ON_BATTERY => PowerState::OnBattery,
      SDL_PowerState::CHARGING => PowerState::Charging,
      SDL_PowerState::CHARGED => PowerState::Charged,
      SDL_PowerState::NO_BATTERY => PowerState::NoBattery,
      _ => PowerState::Unknown,
    },
    percent: if percent < 0 { None } else { Some(percent as u8) },
  }
}

pub fn window_safe_area(window: &sdl3::video::Window) -> SDL_Rect {
  let mut rect = SDL_Rect { x: 0, y: 0, w: 0, h: 0 };
  unsafe { SDL_GetWindowSafeArea(window.raw(), &mut rect) };
  let scale = window_display_scale(window);
  SDL_Rect {
    x: (rect.x as f32 / scale) as i32,
    y: (rect.y as f32 / scale) as i32,
    w: (rect.w as f32 / scale) as i32,
    h: (rect.h as f32 / scale) as i32,
  }
}

pub fn window_display_scale(window: &sdl3::video::Window) -> f32 {
  unsafe { SDL_GetWindowDisplayScale(window.raw()) }
}

pub fn mod_state() -> sdl3::keyboard::Mod {
  unsafe {
    sdl3::keyboard::Mod::from_bits(SDL_GetModState().0)
      .unwrap_or(sdl3::keyboard::Mod::NOMOD)
  }
}
