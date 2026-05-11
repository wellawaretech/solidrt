use sdl3::event::Event;
use sdl3::sys::events::{
  SDL_Event, SDL_EventAction, SDL_PeepEvents, SDL_PumpEvents, SDL_EVENT_FIRST, SDL_EVENT_LAST,
};

/// Flush pending OS input into SDL's internal event queue.
///
/// Must be called from the main thread.
pub fn pump_events() {
  unsafe { SDL_PumpEvents() };
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
