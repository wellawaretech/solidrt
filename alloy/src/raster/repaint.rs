//! Partial repaint on the window surface (okf/done/partial-repaint.md
//! stage 2), raster side: the state deciding how much of a frame must
//! redraw and how it reaches the window. One owner: damage folds in as
//! frames arrive, the drawing frame takes it and asks for its route, and
//! every present outcome reports back - so the ring/age invariants ("no
//! present means the buffer chain is unknown") hold by construction
//! instead of by discipline at each call site.

use super::buffer_age;
use super::{DamageRect, PresentDamage};
use crate::renderer::ISize;
use std::collections::VecDeque;
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};

// Damage-ring depth: how many past frames' deltas are kept for buffer-age
// repairs. A back buffer older than this redraws in full; swapchains run
// 2-4 buffers deep, so 8 leaves margin.
const DAMAGE_RING: usize = 8;

// The size step of a patch drawn through the patch layer, in pixels: the
// patch grows to whole steps, so a patch that breathes by a few pixels
// keeps one layer size.
const PATCH_LAYER_STEP: i32 = 64;

// The share of the window's pixels a patch-layer patch may cover, after
// its growth to whole steps; a larger one draws the whole frame instead.
// It tunes where the patch layer stops paying: the layer adds a pass and
// a second copy of the patch, so a large patch is cheaper drawn whole. A
// starting point, not yet measured on a device.
const PATCH_LAYER_MAX_SHARE: f64 = 0.5;

// How many times the area a patch needs the patch layer may hold before it
// is released and allocated again at the patch's size. It tunes the memory
// one large patch leaves behind against reallocation: 4 lets the layer be
// twice the patch each way (one step either side of a patch that breathes
// by a step stays one layer), and an unused area is invalidated, never
// loaded or stored, so it costs memory, not bandwidth.
const PATCH_LAYER_MAX_OVERSIZE: f64 = 4.0;

// How many patch frames in a row the layer must be oversized for
// (`PATCH_LAYER_MAX_OVERSIZE`) before it shrinks: a patch alternating
// between a small and a large size keeps the large layer instead of
// reallocating every frame, while one large patch stops holding a layer of
// its size after about a second at 60 Hz.
const PATCH_LAYER_SHRINK_FRAMES: u32 = 60;

/// What the window framebuffer is, for the route a frame takes to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WindowBuffer {
  /// Single-sample: FBO 0 of a desktop window, or the headless window
  /// target. A whole frame and a patch both go through the rig.
  SingleSample,
  /// Multisampled FBO 0 (every Android window): a whole frame draws
  /// straight into it (the fast path); a patch goes through the patch
  /// layer, declared to the driver as the frame's damage region, where
  /// the window can declare one.
  Multisampled,
  /// The headless window target under the test switch's layer mode
  /// (`HeadlessPatch::Layer`): a patch goes through the patch layer as on
  /// a multisampled window, with no region to declare (the target is never
  /// presented); a whole frame goes through the rig.
  HeadlessLayer,
}

/// How a frame reaches the window backbuffer. Produced by
/// [`DamageTracker::route`] and consumed by `RasterState::draw_to_window`
/// (the patch layer) and `gl::render_display_list_to_window` (the fast
/// path and the rig), so patch production and patch consumption cannot
/// disagree: a repaint patch exists only inside the route that draws it,
/// by construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WindowRoute {
  /// Multisampled FBO 0 (the Android in-tile fast path): draw the whole
  /// frame straight into the window - no rig pass, no resolve copy. The
  /// renderer clears the surface it draws into, so this route never
  /// carries a patch.
  FastPath,
  /// The patch of a multisampled window (or the headless window target
  /// under the layer switch): the frame inside the patch is drawn into the
  /// patch layer (one layer kept between frames, at least the patch's
  /// size, its patch in the corner, bounded by `patch_layer_fit`) and
  /// copied into the window there, the copy scissored to the patch, over
  /// the preserved back buffer; nothing renders outside it, where the
  /// driver keeps the aged buffer's pixels. The patch is grown to whole
  /// size steps; a zero-sized one draws nothing.
  PatchLayer(DamageRect),
  /// Through the retained rig, resolved 1:1 into FBO 0. `Some` redraws only
  /// the patch rect over the preserved back buffer (a zero-sized rect
  /// redraws nothing; the present still shows the preserved buffer), `None`
  /// the whole window.
  Rig(Option<DamageRect>),
}

impl WindowRoute {
  /// The whole-window redraw on whichever path the backbuffer dictates.
  pub(super) fn whole(buffer: WindowBuffer) -> WindowRoute {
    match buffer {
      WindowBuffer::Multisampled => WindowRoute::FastPath,
      WindowBuffer::SingleSample | WindowBuffer::HeadlessLayer => WindowRoute::Rig(None),
    }
  }

  /// True when the frame redraws a sub-window patch (the partial-present
  /// counter's definition; a zero-sized patch counts - the frame was pruned
  /// to nothing).
  pub(super) fn is_patch(self) -> bool {
    matches!(self, WindowRoute::Rig(Some(_)) | WindowRoute::PatchLayer(_))
  }
}

/// Partial-repaint state for the window. Protocol, in frame order: `fold`
/// damage in as frames arrive (drawn or shed), `take` when a frame draws,
/// `route` for how it reaches the window, then report the outcome -
/// `presented` or `not_presented` - and `invalidated` when the surface is
/// replaced under the frame (rebind retry).
pub(crate) struct DamageTracker {
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
  // Whether a multisampled window's frames may be patched (the damage
  // region is offered and not switched off), or why not: decided with the
  // buffer-age probe, switched off by a refused declaration, or told
  // (`assume_window_patches`, tests). Logged once, at the first frame of a
  // multisampled window after the probe.
  window_patches: Result<(), String>,
  window_patches_logged: bool,
  // A refused declaration was warned about (once per tracker, so once per
  // context: a recovered context starts a fresh tracker).
  refusal_warned: bool,
  // A damage region declared to the driver since the last swap (EGL's
  // frame boundary): set by a declaration the driver took, cleared by a
  // present and by a surface replace.
  declared: bool,
  // A frame ended with a declaration outstanding and unpresented, or the
  // surface was replaced (rebind retry) with one outstanding, which may
  // not have replaced it: the region may still be set, so the frame that
  // draws next is whole and declares nothing, and the buffer it presents
  // may hold undefined pixels outside that region.
  unsettled: bool,
  // Presents so far: the clock a buffer age reads against (the back buffer
  // of age `a` holds present `presents + 1 - a`).
  presents: u64,
  // The present whose buffer may hold undefined pixels: the first one
  // after an unsettled frame. A back buffer holding it, or an older one,
  // is redrawn whole; None once no age the ring can repair reaches it.
  untrusted_through: Option<u64>,
  // Tests: a window surface standing in for EGL's (`fake_surface`).
  #[cfg(test)]
  fake: Option<Rc<RefCell<FakeSurface>>>,
}

/// Tests: a window surface's partial-update facilities without EGL, as
/// `DamageTracker::fake_surface` installs them: the age every frame reads,
/// whether the driver refuses a declaration, and every declaration made.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct FakeSurface {
  /// The back buffer's age the next frame reads.
  pub(crate) age: i32,
  /// Whether the driver refuses the next declarations.
  pub(crate) refuse: bool,
  /// Every declaration made, taken or refused: the rectangle as EGL gets it.
  pub(crate) declared: Vec<(i32, i32, i32, i32)>,
}

impl DamageTracker {
  pub(crate) fn new() -> Self {
    Self {
      pending: PresentDamage::None,
      ring: VecDeque::new(),
      ring_size: ISize::new(0, 0),
      buffer_age: None,
      buffer_age_tried: false,
      window_patches: Err("not probed".to_string()),
      window_patches_logged: false,
      refusal_warned: false,
      declared: false,
      unsettled: false,
      presents: 0,
      untrusted_through: None,
      #[cfg(test)]
      fake: None,
    }
  }

  /// Tests: decide, without EGL, whether a multisampled window can declare
  /// its damage region (what the buffer-age probe decides on a device).
  #[cfg(test)]
  pub(crate) fn assume_window_patches(&mut self, on: bool) {
    self.window_patches = if on { Ok(()) } else { Err("assumed off".to_string()) };
  }

  /// Tests: stand a fake surface in for EGL's, which the frames routed with
  /// no known age then read their age from and declare their region to,
  /// with window patches on. The handle sets the age and the refusal and
  /// reads the declarations back.
  #[cfg(test)]
  pub(crate) fn fake_surface(&mut self) -> Rc<RefCell<FakeSurface>> {
    let fake = Rc::new(RefCell::new(FakeSurface::default()));
    self.fake = Some(fake.clone());
    self.buffer_age_tried = true;
    self.window_patches = Ok(());
    fake
  }

  /// Tests: whether a multisampled window's frames may still be patched.
  #[cfg(test)]
  pub(crate) fn window_patches(&self) -> bool {
    self.window_patches.is_ok()
  }

  /// A frame's content delta joins the pending union (the frame that draws
  /// next carries it, including shed frames' deltas).
  pub(crate) fn fold(&mut self, damage: PresentDamage) {
    self.pending = self.pending.union(damage);
  }

  /// The frame that draws consumes everything pending.
  pub(crate) fn take(&mut self) -> PresentDamage {
    std::mem::replace(&mut self.pending, PresentDamage::None)
  }

  /// The frame's route to the window, and for a patch the region it must
  /// redraw: the frame's own delta unioned with the deltas of every frame
  /// the aged back buffer has not seen (EGL_EXT_buffer_age). `known_age`
  /// is a buffer age the caller knows without asking EGL: the headless
  /// window target, 1 while it holds the last frame and 0 when it holds
  /// none. Any uncertainty - a patch-barred frame (headless without the
  /// patch switch, an active window shader, a window that is read back), a
  /// resize, no buffer age, an age deeper than the ring - answers a
  /// whole-window redraw, so correctness never depends on the extension.
  ///
  /// A patch-layer patch on a window whose age came from EGL is declared
  /// here as the frame's damage region (EGL_KHR_partial_update), after the
  /// age query and before anything draws; a declaration the driver refuses
  /// makes the frame whole and switches window patches off. So the caller
  /// bars the patch (`patch_barred`) for a frame it already knows will not
  /// draw, and reports every frame's outcome: a declaration outstanding at
  /// a frame's end with no present makes the next frames whole until the
  /// buffer chain is repaired. Call once per frame, ahead of its draw.
  pub(crate) fn route(
    &mut self,
    own: PresentDamage,
    size: ISize,
    buffer: WindowBuffer,
    patch_barred: bool,
    known_age: Option<i32>,
  ) -> WindowRoute {
    let whole = WindowRoute::whole(buffer);
    if self.ring_size != size {
      self.ring.clear();
      self.ring_size = size;
      return whole;
    }
    if patch_barred {
      return whole;
    }
    // A region may still be declared from a frame that never reached a
    // swap: this frame draws whole and declares nothing.
    if self.unsettled {
      return whole;
    }
    // Probe before the multisampled escape so the log answers what the
    // APP's EGL context supports even where its window is not patched
    // (okf/backlog/display-list-op-cost.md). Not for an age known without
    // it.
    if known_age.is_none() && !self.buffer_age_tried {
      self.buffer_age_tried = true;
      match buffer_age::BufferAge::new() {
        Ok(query) => {
          log::info!("[alloy] Partial repaint: EGL buffer age available");
          self.window_patches = query.window_patches().map_err(str::to_string);
          self.buffer_age = Some(query);
        }
        Err(e) => {
          log::info!("[alloy] Partial repaint off: {e}");
          self.window_patches = Err("no EGL buffer age".to_string());
        }
      }
    }
    // The capability line, where it applies: the first multisampled frame
    // after the probe (an Android window may bind single-sample first).
    if buffer == WindowBuffer::Multisampled && self.buffer_age_tried && !self.window_patches_logged {
      self.window_patches_logged = true;
      match &self.window_patches {
        Ok(()) => log::info!("[alloy] Multisampled window patches on (EGL_KHR_partial_update)"),
        Err(reason) => log::info!("[alloy] Multisampled window patches off: {reason}"),
      }
    }
    // A multisampled window that cannot declare its damage draws whole;
    // routing it here keeps the partial-present counter honest and skips
    // the patch build.
    if buffer == WindowBuffer::Multisampled && self.window_patches.is_err() {
      return whole;
    }
    if matches!(own, PresentDamage::Full) {
      return whole;
    }
    let age = match known_age.or_else(|| self.surface_age()) {
      Some(age) => age,
      None => return whole,
    };
    if age <= 0 {
      return whole;
    }
    // The back buffer holds a present that may hold undefined pixels, or
    // one older than it.
    if let Some(through) = self.untrusted_through {
      if (self.presents + 1).checked_sub(age as u64).is_none_or(|held| held <= through) {
        return whole;
      }
    }
    // The buffer holds the frame from `age` swaps ago; everything the
    // frames since then changed must redraw, plus this frame's own delta.
    let missing = (age - 1) as usize;
    if missing > self.ring.len() {
      return whole;
    }
    let mut union = own;
    for past in self.ring.iter().rev().take(missing) {
      union = union.union(*past);
    }
    let (width, height) = (size.width as i32, size.height as i32);
    let empty = DamageRect { x: 0, y: 0, width: 0, height: 0 };
    let patch = match union {
      PresentDamage::Full => return whole,
      PresentDamage::None => empty,
      PresentDamage::Rect(r) => match r.clamped(width, height) {
        None => empty,
        Some(c) if c.covers(width, height) => return whole,
        Some(c) => c,
      },
    };
    if buffer == WindowBuffer::SingleSample {
      return WindowRoute::Rig(Some(patch));
    }
    let patch = stepped_patch(patch, width, height);
    if patch.width as f64 * patch.height as f64 > PATCH_LAYER_MAX_SHARE * width as f64 * height as f64 {
      return whole;
    }
    if known_age.is_none() && !self.declare_region(patch, height) {
      return whole;
    }
    WindowRoute::PatchLayer(patch)
  }

  /// The age of the window surface's back buffer, from EGL (or the test
  /// stand-in); None without a buffer-age query.
  fn surface_age(&self) -> Option<i32> {
    #[cfg(test)]
    if let Some(fake) = &self.fake {
      return Some(fake.borrow().age);
    }
    self.buffer_age.as_ref().map(|query| query.age())
  }

  /// Declare a patch-layer frame's `patch` to the driver as its damage
  /// region, on a window `height` pixels high. An empty patch declares one
  /// pixel: no rectangle at all would mean the whole surface. False when
  /// the driver did not take it, and the frame is then whole: the refusal
  /// is warned about once, and unless a declaration of ours was still
  /// outstanding (which `unsettled` keeps from arising: a frame ending with
  /// one makes the next frame whole before it declares), it switches
  /// window patches off for this context.
  fn declare_region(&mut self, patch: DamageRect, height: i32) -> bool {
    let rect = match patch.width > 0 && patch.height > 0 {
      true => (patch.x, height - (patch.y + patch.height), patch.width, patch.height),
      false => (0, 0, 1, 1),
    };
    let error = match self.set_region(rect) {
      Ok(()) => {
        self.declared = true;
        return true;
      }
      Err(error) => error,
    };
    if !self.refusal_warned {
      self.refusal_warned = true;
      log::warn!("[alloy] Damage region refused: {error}");
    }
    if !self.declared {
      log::info!("[alloy] Multisampled window patches off: the driver refused a damage region ({error})");
      self.window_patches = Err(format!("the driver refused a damage region ({error})"));
    }
    false
  }

  /// Hand `rect` to the window surface as the frame's damage region.
  fn set_region(&mut self, rect: (i32, i32, i32, i32)) -> Result<(), String> {
    #[cfg(test)]
    if let Some(fake) = &self.fake {
      let mut fake = fake.borrow_mut();
      fake.declared.push(rect);
      return if fake.refuse { Err("refused by the test".to_string()) } else { Ok(()) };
    }
    match &self.buffer_age {
      Some(query) => query.set_damage_region(rect),
      None => Err("no EGL buffer age".to_string()),
    }
  }

  /// The frame reached the screen: the ring records its content delta -
  /// however much was actually drawn - so a future aged buffer can be
  /// repaired. The swap is the frame boundary: no region is declared any
  /// more, and after an unsettled frame this present is the one whose
  /// buffer is not trusted.
  pub(crate) fn presented(&mut self, own: PresentDamage) {
    self.ring.push_back(own);
    while self.ring.len() > DAMAGE_RING {
      self.ring.pop_front();
    }
    self.presents += 1;
    self.declared = false;
    if self.unsettled {
      self.unsettled = false;
      self.untrusted_through = Some(self.presents);
    }
    // A buffer the ring can repair holds a present at most DAMAGE_RING
    // older than the last one.
    if self.untrusted_through.is_some_and(|through| self.presents > through + DAMAGE_RING as u64) {
      self.untrusted_through = None;
    }
  }

  /// No present: the changes never reached the screen and the buffer
  /// chain's state is uncertain. Carry the delta forward and repaint in
  /// full next time. With a declaration outstanding (no swap ended it) the
  /// frame is unsettled.
  pub(crate) fn not_presented(&mut self, own: PresentDamage) {
    self.pending = self.pending.union(own);
    self.ring.clear();
    if self.declared {
      self.unsettled = true;
    }
  }

  /// The window surface was replaced under a frame (rebind retry): nothing
  /// recorded describes the new buffer chain. A declaration outstanding on
  /// the old surface is gone with it, unless the rebind kept the surface
  /// (nothing tells which), so the frame is unsettled: the retry draws
  /// whole under whatever is still declared.
  pub(crate) fn invalidated(&mut self) {
    self.ring.clear();
    if self.declared {
      self.unsettled = true;
    }
    self.declared = false;
  }
}

/// What the patch layer does for a patch: keep the layer it has, or
/// allocate one of the size given (releasing the old).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LayerFit {
  Keep,
  Allocate(u32, u32),
}

/// The patch layer's fate for a patch of `need` (width, height) pixels,
/// given the layer there is (`layer`, None before the first) and how many
/// patch frames in a row it has been oversized for (`oversized`), with that
/// count for the next frame. A layer too small for the patch grows to hold
/// both (patches alternating between two sizes then reallocate nothing); a
/// layer over `PATCH_LAYER_MAX_OVERSIZE` times the patch's area for
/// `PATCH_LAYER_SHRINK_FRAMES` patch frames in a row is allocated again at
/// the patch's size, so one large patch does not leave a layer of its size
/// behind for good.
pub(crate) fn patch_layer_fit(layer: Option<(u32, u32)>, need: (u32, u32), oversized: u32) -> (LayerFit, u32) {
  let Some((width, height)) = layer else { return (LayerFit::Allocate(need.0, need.1), 0) };
  if width < need.0 || height < need.1 {
    return (LayerFit::Allocate(width.max(need.0), height.max(need.1)), 0);
  }
  let area = |w: u32, h: u32| w as f64 * h as f64;
  if area(width, height) <= PATCH_LAYER_MAX_OVERSIZE * area(need.0, need.1) {
    return (LayerFit::Keep, 0);
  }
  match oversized + 1 >= PATCH_LAYER_SHRINK_FRAMES {
    true => (LayerFit::Allocate(need.0, need.1), 0),
    false => (LayerFit::Keep, oversized + 1),
  }
}

/// `p` grown to whole steps of `PATCH_LAYER_STEP` inside a `width` x
/// `height` window: never smaller than `p` and always inside the window,
/// moved left or up where the growth would cross the right or bottom edge,
/// and cut to the window where it is smaller than a step. An empty patch
/// stays empty.
pub(crate) fn stepped_patch(p: DamageRect, width: i32, height: i32) -> DamageRect {
  if p.width <= 0 || p.height <= 0 {
    return p;
  }
  let grow = |at: i32, len: i32, limit: i32| {
    let want = ((len + PATCH_LAYER_STEP - 1) / PATCH_LAYER_STEP * PATCH_LAYER_STEP).min(limit);
    (at.min(limit - want).max(0), want)
  };
  let (x, w) = grow(p.x, p.width, width);
  let (y, h) = grow(p.y, p.height, height);
  DamageRect { x, y, width: w, height: h }
}
