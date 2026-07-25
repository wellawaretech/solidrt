use std::collections::HashMap;

use alloy::{Modifiers, PointerType};

/// Touch-move resampling against the frame clock (frame-pacing stage 3).
///
/// Android batches touch to the display's vsync, so with the frame signal
/// phase-locked to that same clock (vsync pacing) samples arrive nominally
/// one per frame signal - except when the platform pairs deliveries: nothing
/// at one vsync, two samples at the next (~5x/s on device). Under
/// latest-arrival-wins dispatch that renders as a one-frame stall followed by
/// a double-step.
///
/// SDL's Android path carries no usable sample times (touch is stamped at
/// JNI receipt and historical batch samples are dropped), so instead of
/// timestamp interpolation this models slots: each frame signal is one slot,
/// and consecutive samples are assumed one slot apart - which for
/// vsync-batched delivery they really are, pairs included. Per pointer, per
/// frame:
/// - fresh sample(s) arrived: dispatch the newest. After a bridged gap the
///   pair's second sample lands here as one normal step.
/// - first slot with no fresh sample: bridge the gap by extrapolating one
///   step of the last observed velocity. This is the frame that would stall.
/// - second consecutive empty slot after a bridged gap: the finger stopped,
///   not the delivery; dispatch the real latest position once so state
///   settles truthfully (the bounce is at most the one bridged step), then
///   stay silent until new input.
///
/// Moves feed the history on arrival and dispatch only through sample();
/// down/up stay on arrival (ordering), with down seeding the history so the
/// first move already has a velocity.
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
}

/// One resampled move to dispatch for the current frame.
pub struct Sample {
  pub pointer_type: PointerType,
  pub pointer_id: u64,
  pub x: f32,
  pub y: f32,
  pub modifiers: Modifiers,
}

impl Resampler {
  pub fn new() -> Resampler {
    Resampler { pointers: HashMap::new() }
  }

  /// A pointer went down: seed its history at the contact position. The down
  /// event itself dispatches on arrival; sample() never re-emits it.
  pub fn down(&mut self, key: (PointerType, u64), x: f32, y: f32, modifiers: Modifiers) {
    self
      .pointers
      .insert(key, History { prev: None, latest: (x, y), modifiers, fresh: false, misses: 0, extrapolated: false });
  }

  /// A move arrived: record it for the next sample() call.
  pub fn push(&mut self, key: (PointerType, u64), x: f32, y: f32, modifiers: Modifiers) {
    match self.pointers.get_mut(&key) {
      Some(h) => {
        h.prev = Some(h.latest);
        h.latest = (x, y);
        h.modifiers = modifiers;
        h.fresh = true;
      }
      // Move without a down (missed down, e.g. across an engine swap): track
      // from here; the first slot has no velocity and gaps simply hold.
      None => {
        self
          .pointers
          .insert(key, History { prev: None, latest: (x, y), modifiers, fresh: true, misses: 0, extrapolated: false });
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
        if h.misses == 1 && h.prev.is_some() {
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
        out.push(Sample { pointer_type, pointer_id, x, y, modifiers: h.modifiers });
      }
    }
    out
  }
}
