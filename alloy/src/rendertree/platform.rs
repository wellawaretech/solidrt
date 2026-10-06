use crate::gpu::CoveragePolicy;
use crate::impellers::{Point, Rect, Size};
use crate::rendertree::text::glyphs::{FontSet, TextAtlas};
use crate::rendertree::text::WordCache;
use std::borrow::Cow;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A font to register: raw TTF/OTF bytes plus an optional alias the
/// font registers under instead of its intrinsic family name ("sans", "serif"
/// and "mono" by convention). Alloy ships no font data itself; callers supply
/// fonts (embedded, unpacked from a trailer, read from disk). With none
/// registered, text draws nothing: the glyph engine resolves registered
/// faces only.
#[derive(Clone)]
pub struct FontPayload {
  pub alias: Option<String>,
  pub bytes: Cow<'static, [u8]>,
}

pub struct PlatformContext {
  // The registered fonts as the glyph engine sees them (rendertree/text/
  // glyphs): the one shaper behind the seam and the cell source. Interior
  // mutability so the set can be swapped per app switch (see reset_fonts).
  // Borrowed only on the UI thread (text shaping, the HUD overlay), never
  // across a reset.
  glyphs: RefCell<FontSet>,
  // Shaped words shared by every text (rendertree/text/words.rs); valid for
  // the faces in `glyphs`, so a reset clears it.
  words: RefCell<WordCache>,
  // The one mask atlas every `<text>` draws from; its cells are keyed on
  // face ids of `glyphs`, so a reset voids it.
  text_atlas: RefCell<TextAtlas>,
  // How a text layer turns coverage into color (see CoveragePolicy);
  // settable live for the gallery's by-eye comparison.
  coverage_policy: Cell<CoveragePolicy>,
  window_size: Cell<(f32, f32)>,
  window_size_dirty: Cell<bool>,
  display_scale: Cell<f32>,
  safe_area: Cell<Rect>,
  fps: Cell<u32>,
  // Frame-request latch (Flutter-style scheduleFrame). Atomic and Arc'd because
  // change sources latch from the UI thread (ffi mutations), the alloy event
  // thread (pointer input, resize), and the dev-server connection thread (see
  // go/connection.rs).
  frame_requested: Arc<AtomicBool>,
  // A frame request's caller declared it standing (see
  // declare_standing_demand); consumed by the draw gate after the request.
  standing_demand: AtomicBool,
  // Whether the debug stats overlay (HUD) is drawn. Arc'd so the dev-server
  // connection (a different thread, see go/connection.rs) can toggle it.
  stats_enabled: Arc<AtomicBool>,
}

// Safety: PlatformContext is only used on the UI thread.
unsafe impl Send for PlatformContext {}
unsafe impl Sync for PlatformContext {}

impl PlatformContext {
  pub fn new(fonts: Vec<FontPayload>) -> Self {
    // Startup fonts are the client's own (embedded Notos, a packed trailer);
    // one failing to parse is a build defect, so this panics.
    let glyphs = FontSet::from_payloads(&fonts, |alias, e| panic!("Failed to register font '{alias}': {e}"));
    Self {
      glyphs: RefCell::new(glyphs),
      words: RefCell::new(WordCache::default()),
      text_atlas: RefCell::new(TextAtlas::default()),
      coverage_policy: Cell::new(CoveragePolicy::default()),
      window_size: Cell::new((0.0, 0.0)),
      window_size_dirty: Cell::new(false),
      display_scale: Cell::new(1.0),
      safe_area: Cell::new(Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0))),
      fps: Cell::new(0),
      frame_requested: Arc::new(AtomicBool::new(false)),
      standing_demand: AtomicBool::new(false),
      stats_enabled: Arc::new(AtomicBool::new(false)),
    }
  }

  /// The glyph engine's faces over the registered fonts. UI thread only;
  /// the borrow must not be held across a `reset_fonts`.
  pub fn glyphs(&self) -> Ref<'_, FontSet> {
    self.glyphs.borrow()
  }

  /// The shared word cache. UI thread only, like `glyphs`.
  pub fn words(&self) -> RefMut<'_, WordCache> {
    self.words.borrow_mut()
  }

  /// The text atlas (see `TextAtlas`). UI thread only; borrowed for a
  /// build's `ensure` calls and the frame's `begin_frame`, never across a
  /// `reset_fonts`.
  pub fn text_atlas(&self) -> RefMut<'_, TextAtlas> {
    self.text_atlas.borrow_mut()
  }

  /// The text layers' coverage-to-color policy.
  pub fn coverage_policy(&self) -> CoveragePolicy {
    self.coverage_policy.get()
  }

  /// Change the policy; every text layer re-rasterizes at the frame this
  /// requests.
  pub fn set_coverage_policy(&self, policy: CoveragePolicy) {
    self.coverage_policy.set(policy);
    self.request_frame();
  }

  /// Replace the registered font set (an app switch): a fresh set built
  /// from `fonts` alone, dropping everything previously registered. A font
  /// that fails to parse is skipped with a warning - its role falls back,
  /// same as a missing font file; mid-session this must never panic.
  /// Requests a frame so text reshapes against the new set.
  pub fn reset_fonts(&self, fonts: Vec<FontPayload>) {
    let glyphs = FontSet::from_payloads(&fonts, |alias, e| log::warn!("Could not register font '{alias}': {e}"));
    self.glyphs.replace(glyphs);
    self.words.borrow_mut().clear();
    self.text_atlas.borrow_mut().invalidate_fonts();
    self.request_frame();
  }

  /// Toggle the debug stats overlay. Requests a frame so the change is drawn
  /// even when the app is otherwise idle.
  pub fn set_stats_enabled(&self, enabled: bool) {
    self.stats_enabled.store(enabled, Ordering::Relaxed);
    self.request_frame();
  }

  pub fn stats_enabled(&self) -> bool {
    self.stats_enabled.load(Ordering::Relaxed)
  }

  /// Shared handles for toggling the stats overlay from another thread (the
  /// dev-server connection): set `stats_enabled` and latch `frame_requested`
  /// so the change is drawn even when the app is otherwise idle.
  pub fn stats_handles(&self) -> (Arc<AtomicBool>, Arc<AtomicBool>) {
    (self.stats_enabled.clone(), self.frame_requested.clone())
  }

  /// Shared handle to the frame-request latch, for
  /// AlloyCommand::SetFrameRequestLatch: the platform loop latches it on
  /// surface lifecycle events (expose, resize settling, return to
  /// visibility) so the demand gate repaints without embedder glue.
  pub fn frame_request_handle(&self) -> Arc<AtomicBool> {
    self.frame_requested.clone()
  }

  /// Latch a frame request (Flutter's scheduleFrame). Idempotent; callable
  /// from any thread. The draw gate consumes it via take_frame_requested:
  /// no request, no frame.
  pub fn request_frame(&self) {
    self.frame_requested.store(true, Ordering::Relaxed);
  }

  /// Consume the latch. Called once per render tick (from draw).
  pub fn take_frame_requested(&self) -> bool {
    self.frame_requested.swap(false, Ordering::Relaxed)
  }

  /// Declare standing demand: the caller will want the frame after the one
  /// being built too (an animation loop that re-registers its callback
  /// every frame). The draw gate consumes this frame's request and then,
  /// seeing the declaration, latches the request again
  /// (`take_standing_demand`), so between one gate and the next the latch
  /// says truthfully whether a next frame is wanted - which is what the
  /// raster thread samples at present time to tell a missed present from an
  /// idle gap. Requests made by a one-shot write do not declare it.
  pub fn declare_standing_demand(&self) {
    self.standing_demand.store(true, Ordering::Relaxed);
  }

  /// Consume the standing-demand declaration (the draw gate, after taking
  /// the frame request).
  pub fn take_standing_demand(&self) -> bool {
    self.standing_demand.swap(false, Ordering::Relaxed)
  }

  pub fn window_size(&self) -> (f32, f32) {
    self.window_size.get()
  }

  pub fn set_window_size(&self, width: f32, height: f32) {
    self.window_size.set((width, height));
    self.window_size_dirty.set(true);
    self.request_frame();
  }

  pub fn take_window_size_dirty(&self) -> bool {
    self.window_size_dirty.replace(false)
  }

  pub fn display_scale(&self) -> f32 {
    self.display_scale.get()
  }

  pub fn set_display_scale(&self, scale: f32) {
    self.display_scale.set(scale);
  }

  pub fn safe_area(&self) -> Rect {
    self.safe_area.get()
  }

  pub fn set_safe_area(&self, safe_area: Rect) {
    self.safe_area.set(safe_area);
  }

  pub fn fps(&self) -> u32 {
    self.fps.get()
  }

  pub fn set_fps(&self, fps: u32) {
    self.fps.set(fps);
  }
}
