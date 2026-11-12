//! The platform facts the pump must poll (SDL emits no events for them):
//! screen-keyboard visibility/height, hardware-keyboard presence, power
//! status, and the display refresh rate safety net. One owner: each fact is
//! re-queried on its own cadence and emits an event only on a transition, so
//! consumers see edges instead of level spam.

use std::time::{Duration, Instant};

use crate::display::DEFAULT_REFRESH_HZ;
use crate::event::PlatformEvent;
use crate::window::Window;

// Power poll cadence.
const POWER_CHECK_PERIOD: Duration = Duration::from_secs(10);
// Refresh-rate safety net cadence: catches a rate change the display event
// might miss (e.g. Android 90 <-> 60Hz without a mode-change event).
const REFRESH_CHECK_PERIOD: Duration = Duration::from_secs(1);

pub(super) struct Watch {
  // Screen-keyboard state; transitions emit KeyboardVisibility so the app
  // side can react (auto-blur on hide, layout adjustments on show).
  keyboard_shown: bool,
  keyboard_height: f32,
  // Android hardware-keyboard hotplug arrives via JNI (a Configuration
  // change), not as an SDL keyboard event, so the pump watches the fact and
  // re-emits InputDevices on change. Elsewhere SDL's own added/removed
  // events cover hotplug and this never transitions.
  physical_keyboard: bool,
  last_power_check: Instant,
  last_refresh_check: Instant,
  refresh_rate: f32,
}

// Refresh rate in Hz of the window's current display, with a fallback.
fn display_refresh_rate(window: &Window) -> f32 {
  window.refresh_rate().unwrap_or(DEFAULT_REFRESH_HZ)
}

impl Watch {
  pub(super) fn new(window: &Window) -> Self {
    Watch {
      keyboard_shown: false,
      keyboard_height: 0.0,
      physical_keyboard: crate::window::physical_keyboard(),
      last_power_check: Instant::now(),
      last_refresh_check: Instant::now(),
      refresh_rate: display_refresh_rate(window),
    }
  }

  // Re-query the display mode, emitting on change. The display-event arm
  // and the periodic safety net both funnel here.
  pub(super) fn check_refresh(&mut self, window: &Window) -> Option<PlatformEvent> {
    let hz = display_refresh_rate(window);
    if hz == self.refresh_rate {
      return None;
    }
    self.refresh_rate = hz;
    Some(PlatformEvent::DisplayRefreshRate { hz })
  }

  // One poll per wait; returns the transition events.
  pub(super) fn poll(&mut self, window: &Window) -> Vec<PlatformEvent> {
    let mut events = Vec::new();
    let shown = window.screen_keyboard_shown();
    let scale = window.display_scale();
    let height = crate::window::keyboard_inset_px() as f32 / scale;
    if shown != self.keyboard_shown || height != self.keyboard_height {
      self.keyboard_shown = shown;
      self.keyboard_height = height;
      events.push(PlatformEvent::KeyboardVisibility { shown, height });
    }
    let physical_keyboard = crate::window::physical_keyboard();
    if physical_keyboard != self.physical_keyboard {
      self.physical_keyboard = physical_keyboard;
      events.push(PlatformEvent::InputDevices(crate::window::input_devices()));
    }
    if self.last_power_check.elapsed() >= POWER_CHECK_PERIOD {
      self.last_power_check = Instant::now();
      events.push(PlatformEvent::PowerStatus { info: crate::power::info() });
    }
    if self.last_refresh_check.elapsed() >= REFRESH_CHECK_PERIOD {
      self.last_refresh_check = Instant::now();
      if let Some(e) = self.check_refresh(window) {
        events.push(e);
      }
    }
    events
  }
}
