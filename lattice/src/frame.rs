use std::cell::Cell;
use std::time::Instant;

// Profiling counters, read by the debug overlay. They live as thread-locals on
// the single JS execution thread (where the render handler, setProperty and
// draw all run), so the JS side never makes a timing call: native stamps the
// values around the work and the overlay reads them. Zero added FFI crossings.
thread_local! {
  // Instant captured just before the "render" event is emitted to JS. Read at
  // draw() entry, the delta is the JS render handler (onFrame + flush).
  pub static RENDER_START: Cell<Option<Instant>> = Cell::new(None);
}