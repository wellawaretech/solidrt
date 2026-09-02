//! Partial repaint on the window surface (okf/done/partial-repaint.md
//! stage 2), raster side: the state deciding how much of a frame must
//! redraw and how it reaches the window. One owner: damage folds in as
//! frames arrive, the drawing frame takes it and asks for its route, and
//! every present outcome reports back - so the ring/age invariants ("no
//! present means the buffer chain is unknown") hold by construction
//! instead of by discipline at each call site.

use super::buffer_age;
use super::{DamageRect, PresentDamage};
use impellers::ISize;
use std::collections::VecDeque;

// Damage-ring depth: how many past frames' deltas are kept for buffer-age
// repairs. A back buffer older than this redraws in full; swapchains run
// 2-4 buffers deep, so 8 leaves margin.
const DAMAGE_RING: usize = 8;

/// How a frame reaches the window backbuffer. Produced by
/// [`DamageTracker::route`] and consumed by
/// `gl::render_display_list_to_window`, so patch production and patch
/// consumption cannot disagree: a repaint patch exists only paired with the
/// rig, by construction.
#[derive(Clone, Copy)]
pub(crate) enum WindowRoute {
  /// Multisampled FBO 0 (the Android in-tile fast path): draw straight into
  /// the window - no rig pass, no resolve copy. Impeller clears the wrapped
  /// target and there is no rig to blit from, so this route can never carry
  /// a patch.
  FastPath,
  /// Through the retained rig, resolved 1:1 into FBO 0. `Some` redraws only
  /// the patch rect over the preserved back buffer (a zero-sized rect
  /// redraws nothing; the present still shows the preserved buffer), `None`
  /// the whole window.
  Rig(Option<DamageRect>),
}

impl WindowRoute {
  /// The whole-window redraw on whichever path the backbuffer dictates.
  pub(super) fn whole(fast_path: bool) -> WindowRoute {
    if fast_path {
      WindowRoute::FastPath
    } else {
      WindowRoute::Rig(None)
    }
  }

  /// True when the frame redraws a sub-window patch (the partial-present
  /// counter's definition; a zero-sized patch counts - the frame was pruned
  /// to nothing).
  pub(super) fn is_patch(self) -> bool {
    matches!(self, WindowRoute::Rig(Some(_)))
  }
}

/// Partial-repaint state for the window. Protocol, in frame order: `fold`
/// damage in as frames arrive (drawn or shed), `take` when a frame draws,
/// `route` for how it reaches the window, then report the outcome -
/// `presented` or `not_presented` - and `invalidated` when the surface is
/// replaced under the frame (rebind retry).
pub(super) struct DamageTracker {
  // Damage carried by frames received but not yet drawn: the next frame's
  // own delta plus any load-shed frames', consumed by `take`.
  pending: PresentDamage,
  // Per presented frame, that frame's content delta (newest last): what a
  // later frame must redraw over an aged back buffer. Cleared on resize and
  // on any failed present (the buffer's content is then unknown).
  ring: VecDeque<PresentDamage>,
  // The surface size the ring's entries were recorded at.
  ring_size: ISize,
  // The EGL buffer-age query; probed once at the first eligible frame
  // (None afterwards means unavailable, logged then).
  buffer_age: Option<buffer_age::BufferAge>,
  buffer_age_tried: bool,
}

impl DamageTracker {
  pub(super) fn new() -> Self {
    Self {
      pending: PresentDamage::None,
      ring: VecDeque::new(),
      ring_size: ISize::new(0, 0),
      buffer_age: None,
      buffer_age_tried: false,
    }
  }

  /// A frame's content delta joins the pending union (the frame that draws
  /// next carries it, including shed frames' deltas).
  pub(super) fn fold(&mut self, damage: PresentDamage) {
    self.pending = self.pending.union(damage);
  }

  /// The frame that draws consumes everything pending.
  pub(super) fn take(&mut self) -> PresentDamage {
    std::mem::replace(&mut self.pending, PresentDamage::None)
  }

  /// The frame's route to the window, and for the rig the region it must
  /// redraw: the frame's own delta unioned with the deltas of every frame
  /// the aged back buffer has not seen (EGL_EXT_buffer_age). Any
  /// uncertainty - a patch-barred frame (playback, an active window
  /// shader), a resize, no buffer age, an age deeper than the ring -
  /// answers a whole-window redraw, so correctness never depends on the
  /// extension.
  pub(super) fn route(&mut self, own: PresentDamage, size: ISize, fast_path: bool, patch_barred: bool) -> WindowRoute {
    if self.ring_size != size {
      self.ring.clear();
      self.ring_size = size;
      return WindowRoute::whole(fast_path);
    }
    if patch_barred {
      return WindowRoute::whole(fast_path);
    }
    // Probe before the fast-path escape so the log answers what the APP's
    // EGL context supports even on devices where partial repaint stays off
    // (okf/backlog/display-list-op-cost.md).
    if !self.buffer_age_tried {
      self.buffer_age_tried = true;
      match buffer_age::BufferAge::new() {
        Ok(query) => {
          log::info!("[alloy] partial repaint: EGL buffer age available");
          self.buffer_age = Some(query);
        }
        Err(e) => log::info!("[alloy] partial repaint off: {e}"),
      }
    }
    // The multisampled-FBO0 fast path draws straight into the window and
    // cannot patch; routing it here keeps the partial-present counter
    // honest and skips the wrapper-list build.
    if fast_path {
      return WindowRoute::FastPath;
    }
    if matches!(own, PresentDamage::Full) {
      return WindowRoute::Rig(None);
    }
    let age = match self.buffer_age.as_ref() {
      Some(query) => query.age(),
      None => return WindowRoute::Rig(None),
    };
    if age <= 0 {
      return WindowRoute::Rig(None);
    }
    // The buffer holds the frame from `age` swaps ago; everything the
    // frames since then changed must redraw, plus this frame's own delta.
    let missing = (age - 1) as usize;
    if missing > self.ring.len() {
      return WindowRoute::Rig(None);
    }
    let mut union = own;
    for past in self.ring.iter().rev().take(missing) {
      union = union.union(*past);
    }
    let empty = DamageRect { x: 0, y: 0, width: 0, height: 0 };
    let patch = match union {
      PresentDamage::Full => None,
      PresentDamage::None => Some(empty),
      PresentDamage::Rect(r) => match r.clamped(size.width as i32, size.height as i32) {
        None => Some(empty),
        Some(c) if c.covers(size.width as i32, size.height as i32) => None,
        Some(c) => Some(c),
      },
    };
    WindowRoute::Rig(patch)
  }

  /// The frame reached the screen: the ring records its content delta -
  /// however much was actually drawn - so a future aged buffer can be
  /// repaired.
  pub(super) fn presented(&mut self, own: PresentDamage) {
    self.ring.push_back(own);
    while self.ring.len() > DAMAGE_RING {
      self.ring.pop_front();
    }
  }

  /// No present: the changes never reached the screen and the buffer
  /// chain's state is uncertain. Carry the delta forward and repaint in
  /// full next time.
  pub(super) fn not_presented(&mut self, own: PresentDamage) {
    self.pending = self.pending.union(own);
    self.ring.clear();
  }

  /// The window surface was replaced under a frame (rebind retry): nothing
  /// recorded describes the new buffer chain.
  pub(super) fn invalidated(&mut self) {
    self.ring.clear();
  }
}
