//! The device's power: whether it runs on battery, charges or has no
//! battery, and the charge left. `SDL_GetPowerInfo` needs no window and no
//! SDL_Init, but it is not thread safe, so one caller at a time holds the
//! lock. Each call reads the platform again; a change event comes with the
//! event pump.

use std::ffi::c_int;
use std::sync::{Mutex, PoisonError};

use sdl3::sys::power::{SDL_GetPowerInfo, SDL_PowerState};

static LOCK: Mutex<()> = Mutex::new(());

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
  /// The charge left, 0 to 100; none when the platform does not say.
  pub percent: Option<u8>,
}

/// The power state and the charge left, as the platform reports them now.
pub fn info() -> PowerInfo {
  let _one_caller = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
  // The seconds left are read and ignored: state and percentage only.
  let mut seconds: c_int = -1;
  let mut percent: c_int = -1;
  // SAFETY: both out-pointers point to live locals for the whole call;
  // the lock above keeps SDL's non-thread-safe query to one caller.
  let state = unsafe { SDL_GetPowerInfo(&mut seconds, &mut percent) };
  PowerInfo {
    state: match state {
      SDL_PowerState::ON_BATTERY => PowerState::OnBattery,
      SDL_PowerState::CHARGING => PowerState::Charging,
      SDL_PowerState::CHARGED => PowerState::Charged,
      SDL_PowerState::NO_BATTERY => PowerState::NoBattery,
      _ => PowerState::Unknown,
    },
    percent: u8::try_from(percent).ok(),
  }
}
