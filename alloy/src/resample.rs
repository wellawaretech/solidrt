use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{AlloyEvent, Modifiers, PointerType};

/// Pointer-move resampling against the frame clock: ALL pointer types buffer
/// their moves here and dispatch one position per pointer per frame slot, so
/// every move a frame delivers is the same age (frame-batched delivery; the
/// runtime emits the "pointerFrame" terminator after the batch).
///
/// Touch additionally bridges delivery gaps. Android batches touch to the
/// display's vsync, so with the frame signal phase-locked to that same clock
/// (vsync pacing) samples arrive nominally one per frame signal - except
/// when the platform pairs deliveries: nothing at one vsync, two samples at
/// the next (~5x/s on device). Under latest-arrival-wins dispatch that
/// renders as a one-frame stall followed by a double-step.
///
/// SDL's Android path carries no usable sample times (touch is stamped at
/// JNI receipt and historical batch samples are dropped), so instead of
/// timestamp interpolation this models slots: each frame signal is one slot,
/// and consecutive samples are assumed one slot apart - which for
/// vsync-batched delivery they really are, pairs included. Per touch
/// pointer, per frame:
/// - fresh sample(s) arrived: dispatch the newest. After a bridged gap the
///   pair's second sample lands here as one normal step.
/// - first slot with no fresh sample: bridge the gap by extrapolating one
///   step of the last observed velocity. This is the frame that would stall.
/// - second consecutive empty slot after a bridged gap: the finger stopped,
///   not the delivery; dispatch the real latest position once so state
///   settles truthfully (the bounce is at most the one bridged step), then
///   stay silent until new input.
///
/// Mouse and pen never extrapolate: desktop delivery has no paired-vsync gap
/// to bridge, and a bridged step would fake an overshoot every time the
/// device stops. They dispatch the latest buffered position per slot, which
/// also bounds a high-polling-rate mouse to one hit test and JS dispatch per
/// frame.
///
/// Moves feed the history on arrival and dispatch only through sample();
/// down/up stay on arrival (ordering), with down re-seeding the history so a
/// buffered pre-down move collapses into the down instead of dispatching
/// stale after it (and, for touch, so the first move has a velocity).
pub struct Resampler {
  pointers: HashMap<(PointerType, u64), History>,
}

struct History {
  // None until a second position is known; no velocity, so gaps hold.
  prev: Option<(f32, f32)>,
  latest: (f32, f32),
  modifiers: Modifiers,
  // A sample arrived since the last sample() call.
  fresh: bool,
  // Consecutive sample() calls that found nothing fresh.
  misses: u32,
  // The last dispatched position was extrapolated, so a stop must settle
  // back to `latest`.
  extrapolated: bool,
  // Movement since the last dispatch. Hardware deltas (mouse xrel/yrel) sum
  // here on push - positions collapse to the latest per slot, deltas must
  // accumulate or fast motion silently loses distance - and survive the
  // down() re-seed (the motion happened; a click mid-flick must not eat it).
  rel: (f32, f32),
  // This pointer reports hardware deltas (sticky from the first push that
  // carries one). Pointers without them (touch, synthetic) report movement
  // as the dispatched-position diff instead, so movement mirrors position
  // exactly - including the extrapolation bounce.
  hw: bool,
  // The last dispatched position, the baseline for derived movement. Seeded
  // by down() at the contact so the first move's movement is contact-based.
  dispatched: Option<(f32, f32)>,
}

impl History {
  // Fresh tracking state at a position: nothing buffered, no velocity, no
  // movement baseline. Callers adjust the fields their entry point implies.
  fn new(x: f32, y: f32, modifiers: Modifiers) -> History {
    History {
      prev: None,
      latest: (x, y),
      modifiers,
      fresh: false,
      misses: 0,
      extrapolated: false,
      rel: (0.0, 0.0),
      hw: false,
      dispatched: None,
    }
  }

  // A down re-seeds the history at the contact. Position tracking restarts
  // (no velocity, nothing buffered, movement baselined at the contact);
  // accumulated relative motion and the hw fact persist - the motion
  // physically happened, and a click mid-flick must not shorten a
  // mouse-look turn. A new field must decide here which side it is on.
  fn re_seed(&mut self, x: f32, y: f32, modifiers: Modifiers) {
    self.prev = None;
    self.latest = (x, y);
    self.modifiers = modifiers;
    self.fresh = false;
    self.misses = 0;
    self.extrapolated = false;
    self.dispatched = Some((x, y));
  }
}

/// One resampled move to dispatch for the current frame. `dx`/`dy` is the
/// movement since the previous dispatch: summed hardware deltas for pointers
/// that report them (the only motion signal in relative mouse mode, where
/// positions freeze), the dispatched-position diff otherwise.
pub struct Sample {
  pub pointer_type: PointerType,
  pub pointer_id: u64,
  pub x: f32,
  pub y: f32,
  pub dx: f32,
  pub dy: f32,
  pub modifiers: Modifiers,
}

impl Resampler {
  pub fn new() -> Resampler {
    Resampler { pointers: HashMap::new() }
  }

  /// A pointer went down: seed its history at the contact position. The down
  /// event itself dispatches on arrival; sample() never re-emits it.
  pub fn down(&mut self, key: (PointerType, u64), x: f32, y: f32, modifiers: Modifiers) {
    match self.pointers.get_mut(&key) {
      Some(h) => h.re_seed(x, y, modifiers),
      None => {
        let mut h = History::new(x, y, modifiers);
        h.dispatched = Some((x, y));
        self.pointers.insert(key, h);
      }
    }
  }

  /// A move arrived: record it for the next sample() call. `rel` is the
  /// hardware motion delta when the device reports one; it sums into the
  /// history (positions collapse per slot, deltas must accumulate).
  pub fn push(&mut self, key: (PointerType, u64), x: f32, y: f32, rel: Option<(f32, f32)>, modifiers: Modifiers) {
    match self.pointers.get_mut(&key) {
      Some(h) => {
        h.prev = Some(h.latest);
        h.latest = (x, y);
        h.modifiers = modifiers;
        h.fresh = true;
        if let Some((dx, dy)) = rel {
          h.rel.0 += dx;
          h.rel.1 += dy;
          h.hw = true;
        }
      }
      // Move without a down (missed down, e.g. across an engine swap): track
      // from here; the first slot has no velocity and gaps simply hold.
      None => {
        let mut h = History::new(x, y, modifiers);
        h.fresh = true;
        if let Some((dx, dy)) = rel {
          h.rel = (dx, dy);
          h.hw = true;
        }
        self.pointers.insert(key, h);
      }
    }
  }

  /// The pointer ended; the up event carries the final position on arrival.
  pub fn remove(&mut self, key: (PointerType, u64)) {
    self.pointers.remove(&key);
  }

  pub fn clear(&mut self) {
    self.pointers.clear();
  }

  /// Advance one frame slot: the moves to dispatch for this frame.
  pub fn sample(&mut self) -> Vec<Sample> {
    let mut out = Vec::new();
    for (&(pointer_type, pointer_id), h) in self.pointers.iter_mut() {
      let position = if h.fresh {
        h.fresh = false;
        h.misses = 0;
        h.extrapolated = false;
        Some(h.latest)
      } else {
        h.misses = h.misses.saturating_add(1);
        // Touch-only: mouse/pen must not overshoot on stop (see module doc).
        if h.misses == 1 && h.prev.is_some() && pointer_type == PointerType::Touch {
          let (px, py) = h.prev.expect("prev checked above");
          let (lx, ly) = h.latest;
          h.extrapolated = true;
          Some((lx + (lx - px), ly + (ly - py)))
        } else if h.misses == 2 && h.extrapolated {
          h.extrapolated = false;
          Some(h.latest)
        } else {
          None
        }
      };
      if let Some((x, y)) = position {
        // Movement: drain the summed hardware deltas when the pointer has
        // them (positions freeze in relative mouse mode, so the position
        // diff would read 0); otherwise diff against the last dispatched
        // position, so derived movement bounces exactly when positions do.
        let (dx, dy) = if h.hw {
          std::mem::take(&mut h.rel)
        } else {
          let (bx, by) = h.dispatched.unwrap_or((x, y));
          (x - bx, y - by)
        };
        h.dispatched = Some((x, y));
        out.push(Sample { pointer_type, pointer_id, x, y, dx, dy, modifiers: h.modifiers });
      }
    }
    out
  }
}

/// Shared handle onto the process's one Resampler. Feeding is a
/// producer-side duty: whoever emits pointer events feeds the histories at
/// emission - the platform loop for real input (moves are consumed there and
/// never travel as events; downs seed and ups drop the history before their
/// events are sent), synthetic-input producers at their own send sites,
/// following the same rule. The single UI consumer samples per frame slot
/// and clears across engine swaps. Cheap to clone.
#[derive(Clone)]
pub struct SharedResampler(Arc<Mutex<Resampler>>);

impl SharedResampler {
  pub fn new() -> SharedResampler {
    SharedResampler(Arc::new(Mutex::new(Resampler::new())))
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Resampler> {
    self.0.lock().expect("resampler lock poisoned")
  }

  pub fn down(&self, key: (PointerType, u64), x: f32, y: f32, modifiers: Modifiers) {
    self.lock().down(key, x, y, modifiers)
  }

  pub fn push(&self, key: (PointerType, u64), x: f32, y: f32, rel: Option<(f32, f32)>, modifiers: Modifiers) {
    self.lock().push(key, x, y, rel, modifiers)
  }

  pub fn remove(&self, key: (PointerType, u64)) {
    self.lock().remove(key)
  }

  pub fn clear(&self) {
    self.lock().clear()
  }

  pub fn sample(&self) -> Vec<Sample> {
    self.lock().sample()
  }

  /// The producer-side duty as one call: feed a pointer event into the
  /// histories at its send site. Returns true for moves, which are consumed
  /// here and must not travel as events (the frame consumer samples and
  /// dispatches them); downs and ups update the histories and still travel,
  /// dispatching on arrival.
  pub fn feed(&self, event: &AlloyEvent) -> bool {
    match event {
      AlloyEvent::PointerMove { pointer_id, pointer_type, x, y, rel, modifiers } => {
        self.push((*pointer_type, *pointer_id), *x, *y, *rel, *modifiers);
        true
      }
      AlloyEvent::PointerDown { pointer_id, pointer_type, x, y, modifiers, .. } => {
        self.down((*pointer_type, *pointer_id), *x, *y, *modifiers);
        false
      }
      AlloyEvent::PointerUp { pointer_id, pointer_type, .. } => {
        self.remove((*pointer_type, *pointer_id));
        false
      }
      _ => false,
    }
  }
}
