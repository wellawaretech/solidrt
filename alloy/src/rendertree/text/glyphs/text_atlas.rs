// The text atlas: the one mask atlas every `<text>` draws from
// (okf/plans/text-own-rasterizer.md, stage 2). A cache,
// not a store: a text layer keeps pixels and never cell references, so a
// cell evicted or made at a stale display scale is only a miss at the
// layer's next rasterization, and remaking is the same path as making.
// Three sources fill it:
//
// 1. Warm-up on the worker: printable ASCII at every subpixel phase for a
//    style the moment a build first sees it (`warm`), and whatever an app
//    warms ahead of time (its type scale at startup, under the splash).
// 2. The synchronous path (`ensure`): a glyph a build needs now and the
//    atlas lacks is made on this thread, in paint order, until the frame's
//    budget is spent - time, not a count, so a slow CPU makes fewer.
// 3. Past the budget the worker makes the rest at `Needed` priority, ahead
//    of any warm-up; the layer draws without them and re-rasterizes the
//    frame they land (`begin_frame` says when).
//
// Cells are keyed on the face, the device pixels per em, the weight, the
// style and the subpixel phase besides the glyph, so one texture serves
// every style on screen and a glyph pass binds once per layer. Frames are
// stamped on every use so a full atlas evicts what nobody drew lately.
use super::atlas::{AtlasPacker, CellPlacement, GlyphAtlas, InsertOutcome};
use super::cells::{CellKind, CellRequest, Rasterizer};
use super::fonts::{Face, FaceId, FontSet};
use super::worker::{CellJob, CellWorker, JobPriority};
use crate::gpu::{SamplerFilter, SamplerState, SamplerWrap, MIN_ANISOTROPY};
use crate::Context;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Subpixel phases per glyph along x: a cell per third of a pixel, the
/// spike's choice (Skia positions at quarters).
pub const PHASES: u8 = 3;
/// The synchronous path's budget per frame: cells made on the UI thread at
/// build time stop once this much of the frame went to them (a few cells
/// on an armv7 TV at a millisecond each, dozens on a desktop); the rest
/// go to the worker.
const SYNC_CELL_BUDGET: Duration = Duration::from_millis(3);
/// Frames a cell may go unused before a full atlas evicts it: two seconds
/// at 60 Hz, so a screen that comes right back keeps its cells.
const EVICT_AFTER_FRAMES: u64 = 120;
/// The code points a warm-up makes for a style: printable ASCII.
const WARM_FIRST: u32 = 0x20;
const WARM_LAST: u32 = 0x7e;
/// Below this display scale masks are stem-darkened by `LOW_DPI_DARKEN_EM`
/// of the em per side: the light-on-dark bleed the plan's symptom section
/// describes lives on 1x desktops. Zero until stage 2's step 4 tunes it
/// together with retiring the Medium default weight.
const LOW_DPI_SCALE: f32 = 2.0;
const LOW_DPI_DARKEN_EM: f32 = 0.0;
/// The thread the text atlas's cells are made on.
const WORKER_NAME: &str = "alloy-text-cells";
const ATLAS_LABEL: &str = "text-atlas";

/// One face at one size, weight, width and style on one kind of display:
/// what a run's glyphs are made as. `ppem` is the device pixels per em
/// (the run's size times the layer's scale) and `stretch` the width axis
/// percentage, both kept as the float's bits so equal values make equal
/// keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleKey {
  pub face: FaceId,
  ppem: u32,
  pub weight: u16,
  stretch: u32,
  pub italic: bool,
  /// Low-DPI stem darkening applies (the display scale is below
  /// `LOW_DPI_SCALE`): the same ppem reads differently at 1x and 2x.
  pub darken: bool,
}

impl StyleKey {
  pub fn new(face: FaceId, ppem: f32, weight: u16, stretch: f32, italic: bool, display_scale: f32) -> Self {
    Self {
      face,
      ppem: ppem.to_bits(),
      weight,
      stretch: stretch.to_bits(),
      italic,
      darken: display_scale < LOW_DPI_SCALE,
    }
  }

  pub fn ppem(&self) -> f32 {
    f32::from_bits(self.ppem)
  }

  pub fn stretch(&self) -> f32 {
    f32::from_bits(self.stretch)
  }
}

/// A cell's identity in the text atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellKey {
  pub style: StyleKey,
  pub phase: u8,
  pub glyph: u16,
}

/// Where a glyph's device x lands: the pixel its cell's origin snaps to
/// and the phase its cell is made at.
pub fn split_phase(x: f32) -> (i32, u8) {
  let floor = x.floor();
  let phase = (((x - floor) * PHASES as f32) as u8).min(PHASES - 1);
  (floor as i32, phase)
}

/// A style an app asked to warm ahead of its use (`request_warm`): the face
/// and the size in logical pixels, resolved to a style key at the next
/// frame, when the display scale is known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarmRequest {
  pub face: FaceId,
  pub size: f32,
  pub weight: u16,
  pub stretch: f32,
  pub italic: bool,
}

// A job on the worker, by its owner id: which cells it makes.
struct Job {
  style: StyleKey,
  phase: u8,
}

pub struct TextAtlas {
  // Created on first use: the texture needs the GPU context, which the
  // platform context does not hold.
  atlas: Option<GlyphAtlas<CellKey>>,
  worker: Option<CellWorker>,
  worker_failed: bool,
  // The synchronous path's rasterizer: this thread's own swash state.
  rasterizer: Rasterizer,
  jobs: HashMap<u64, Job>,
  next_job: u64,
  // Cells on the worker, and cells the font cannot make (or the atlas
  // refused): neither is asked for twice.
  pending: HashSet<CellKey>,
  // Jobs the worker has not finished, counted down on the worker thread
  // itself: what a headless host's settle waits on. It must not wait on
  // the landing, which only a frame does, and a frame only runs once
  // nothing is in flight.
  in_flight: Arc<AtomicU32>,
  failed: HashSet<CellKey>,
  warmed: HashSet<StyleKey>,
  // Styles asked for ahead of use, warmed at the next frame start.
  warm_requests: Vec<WarmRequest>,
  frame: u64,
  // Where this frame's synchronous budget ends.
  deadline: Option<Instant>,
  // Cells landed since the last `begin_frame`.
  landed: bool,
  // Registered fonts changed: the face ids in every key are void.
  stale: bool,
  // What a landing job does on the worker thread: latch a frame and wake
  // the loop, so the frame that completes the layers runs.
  frame_request: Option<Arc<AtomicBool>>,
  wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for TextAtlas {
  fn default() -> Self {
    Self {
      atlas: None,
      worker: None,
      worker_failed: false,
      rasterizer: Rasterizer::default(),
      jobs: HashMap::new(),
      next_job: 1,
      pending: HashSet::new(),
      in_flight: Arc::new(AtomicU32::new(0)),
      failed: HashSet::new(),
      warmed: HashSet::new(),
      warm_requests: Vec::new(),
      frame: 0,
      deadline: None,
      landed: false,
      stale: false,
      frame_request: None,
      wake: None,
    }
  }
}

impl TextAtlas {
  /// Start a frame: land the cells the worker made (uploading them), open
  /// this frame's synchronous budget, and queue the warm-ups asked for
  /// since the last frame at `display_scale`. `frame_request` is the
  /// platform's latch, which a landing job sets from the worker thread.
  pub fn begin_frame(&mut self, ctx: &Context, frame_request: &Arc<AtomicBool>, fonts: &FontSet, display_scale: f32) {
    if self.frame_request.is_none() {
      self.frame_request = Some(frame_request.clone());
      self.wake = ctx.frame_wake();
    }
    self.frame += 1;
    if let Some(atlas) = &mut self.atlas {
      atlas.packer_mut().begin_frame(self.frame);
    }
    self.deadline = Some(Instant::now() + SYNC_CELL_BUDGET);
    self.landed = false;
    self.land(ctx);
    for request in std::mem::take(&mut self.warm_requests) {
      let key = StyleKey::new(
        request.face,
        request.size * display_scale,
        request.weight,
        request.stretch,
        request.italic,
        display_scale,
      );
      if !self.warmed.contains(&key) {
        self.warm(fonts, key);
      }
    }
  }

  /// Ask for a style's printable ASCII ahead of its use; made at the next
  /// frame start on the worker. A style already warmed costs nothing.
  pub fn request_warm(&mut self, request: WarmRequest) {
    if !self.warm_requests.contains(&request) {
      self.warm_requests.push(request);
    }
  }

  /// Whether cells landed at this frame's start: every incomplete layer
  /// then re-rasterizes.
  pub fn landed(&self) -> bool {
    self.landed
  }

  /// The frames begun so far: what a text layer stamps its last use with.
  pub fn frame(&self) -> u64 {
    self.frame
  }

  /// The atlas texture, once a glyph was asked for.
  pub fn texture(&self) -> Option<u64> {
    self.atlas.as_ref().map(|a| a.texture())
  }

  /// The atlas's current size in texels.
  pub fn size(&self) -> Option<(u32, u32)> {
    self.atlas.as_ref().map(|a| a.packer().size())
  }

  /// Jobs the worker has not finished: work in flight a layer waits on
  /// (their cells land at the frame the finish requests).
  pub fn pending(&self) -> usize {
    self.in_flight.load(Ordering::Relaxed) as usize
  }

  /// Cells asked of the worker that have not landed.
  pub fn queued_cells(&self) -> usize {
    self.pending.len()
  }

  /// The registered fonts changed (an app switch): every cell is void.
  /// The texture is kept and cleared at the next use.
  pub fn invalidate_fonts(&mut self) {
    self.stale = true;
    self.warmed.clear();
    self.failed.clear();
  }

  /// The placements of `glyphs` for `style` at `phase`, making what the
  /// atlas lacks: synchronously within the frame's budget, on the worker
  /// past it (a None in the result, filled in at a later frame). Also
  /// warms the style's ASCII on first sight. Returns how many came back
  /// None.
  pub fn ensure(
    &mut self,
    ctx: &Context,
    fonts: &FontSet,
    style: StyleKey,
    phase: u8,
    glyphs: &[u16],
    out: &mut Vec<Option<CellPlacement>>,
  ) -> usize {
    out.clear();
    if self.open(ctx).is_none() {
      out.resize(glyphs.len(), None);
      return glyphs.len();
    }
    let Some(face) = fonts.face(style.face) else {
      out.resize(glyphs.len(), None);
      return glyphs.len();
    };
    let mut misses: Vec<u16> = Vec::new();
    for &glyph in glyphs {
      let key = CellKey { style, phase, glyph };
      let atlas = self.atlas.as_mut().expect("opened above");
      match atlas.packer().placement(key) {
        Some(placement) => {
          atlas.packer_mut().touch(key);
          out.push(Some(placement));
        }
        None => {
          if !self.failed.contains(&key) && !self.pending.contains(&key) && !misses.contains(&glyph) {
            misses.push(glyph);
          }
          out.push(None);
        }
      }
    }
    if !self.warmed.contains(&style) {
      self.warmed.insert(style);
      self.warm(fonts, style);
    }
    if misses.is_empty() {
      return out.iter().filter(|p| p.is_none()).count();
    }
    // Within the budget the misses are made here and now; past it they go
    // to the worker ahead of every warm-up.
    let in_budget = self.deadline.is_some_and(|deadline| Instant::now() < deadline);
    if in_budget {
      let request = request(face, style, phase, misses.clone());
      let bytes = face.bytes().clone();
      let made = self.rasterizer.rasterize(bytes.as_ref().as_ref(), &request).unwrap_or_default();
      let mut inserted = 0;
      for cell in made {
        let key = CellKey { style, phase, glyph: cell.key };
        let atlas = self.atlas.as_mut().expect("opened above");
        if atlas.insert(cell.with_key(key)) == InsertOutcome::Full {
          self.failed.insert(key);
        } else {
          inserted += 1;
        }
      }
      for &glyph in &misses {
        let key = CellKey { style, phase, glyph };
        let atlas = self.atlas.as_ref().expect("opened above");
        if atlas.packer().placement(key).is_none() {
          self.failed.insert(key);
        }
      }
      if inserted > 0 {
        self.flush(ctx);
      }
      // Fill in what was just made.
      for (slot, &glyph) in out.iter_mut().zip(glyphs) {
        if slot.is_none() {
          let key = CellKey { style, phase, glyph };
          let atlas = self.atlas.as_mut().expect("opened above");
          if let Some(placement) = atlas.packer().placement(key) {
            atlas.packer_mut().touch(key);
            *slot = Some(placement);
          }
        }
      }
    } else {
      self.submit(face, style, phase, misses, JobPriority::Needed);
    }
    out.iter().filter(|p| p.is_none()).count()
  }

  /// Make `style`'s printable ASCII at every phase on the worker, ahead of
  /// its use. A style already warmed is a no-op.
  pub fn warm(&mut self, fonts: &FontSet, style: StyleKey) {
    let Some(face) = fonts.face(style.face) else { return };
    self.warmed.insert(style);
    let glyphs: Vec<u16> = (WARM_FIRST..=WARM_LAST)
      .filter_map(char::from_u32)
      .filter_map(|ch| face.glyph_id(ch))
      .collect::<HashSet<u16>>()
      .into_iter()
      .collect();
    for phase in 0..PHASES {
      let missing: Vec<u16> = glyphs
        .iter()
        .copied()
        .filter(|&glyph| {
          let key = CellKey { style, phase, glyph };
          !self.pending.contains(&key)
            && !self.failed.contains(&key)
            && self.atlas.as_ref().is_none_or(|a| a.packer().placement(key).is_none())
        })
        .collect();
      if !missing.is_empty() {
        self.submit(face, style, phase, missing, JobPriority::Warm);
      }
    }
  }

  // Create the atlas texture on first use, or clear it after a font reset.
  // None when the GPU refused the texture (logged once).
  fn open(&mut self, ctx: &Context) -> Option<()> {
    if self.stale {
      self.stale = false;
      self.pending.clear();
      self.jobs.clear();
      if let Some(atlas) = &mut self.atlas {
        atlas.packer_mut().clear();
        atlas.packer_mut().begin_frame(self.frame);
      }
    }
    if self.atlas.is_none() {
      let max_side = ctx.gpu_limits().max_texture_size;
      let mut packer = AtlasPacker::new(CellKind::Mask { ppem: 0.0 }, max_side).with_eviction(EVICT_AFTER_FRAMES);
      packer.begin_frame(self.frame);
      let sampler = SamplerState {
        filter: SamplerFilter::Linear,
        wrap: SamplerWrap::Clamp,
        mipmap: false,
        anisotropy: MIN_ANISOTROPY,
      };
      match GlyphAtlas::new(ctx, packer, sampler, ATLAS_LABEL) {
        Ok(atlas) => self.atlas = Some(atlas),
        Err(e) => {
          if !self.worker_failed {
            log::warn!("[text] the text atlas texture could not be created: {e}");
            self.worker_failed = true;
          }
          return None;
        }
      }
    }
    Some(())
  }

  fn submit(&mut self, face: &Face, style: StyleKey, phase: u8, glyphs: Vec<u16>, priority: JobPriority) {
    if self.worker.is_none() && !self.worker_failed {
      match CellWorker::spawn(WORKER_NAME) {
        Ok(worker) => self.worker = Some(worker),
        Err(e) => {
          log::warn!("[text] the glyph worker did not start: {e}; cells are made on the UI thread only");
          self.worker_failed = true;
        }
      }
    }
    let Some(worker) = &self.worker else { return };
    let owner = self.next_job;
    self.next_job += 1;
    let request = request(face, style, phase, glyphs.clone());
    let latch = self.frame_request.clone();
    let wake = self.wake.clone();
    let in_flight = self.in_flight.clone();
    in_flight.fetch_add(1, Ordering::SeqCst);
    let done: Box<dyn FnOnce() + Send> = Box::new(move || {
      // The cells are queued for the landing by now: the job is no longer
      // in flight, and the frame that lands them is requested.
      in_flight.fetch_sub(1, Ordering::SeqCst);
      if let Some(latch) = latch {
        latch.store(true, Ordering::Relaxed);
      }
      if let Some(wake) = wake {
        wake();
      }
    });
    let job = CellJob { owner, bytes: face.bytes().clone(), request, priority, done: Some(done) };
    if worker.submit(job) {
      self.jobs.insert(owner, Job { style, phase });
      for glyph in glyphs {
        self.pending.insert(CellKey { style, phase, glyph });
      }
    } else {
      // Dropped unsent: its closure never runs.
      self.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
  }

  // Land every finished job in the atlas and upload.
  fn land(&mut self, ctx: &Context) {
    let done = self.worker.as_ref().map(|w| w.drain()).unwrap_or_default();
    if done.is_empty() {
      return;
    }
    let mut inserted = 0;
    for batch in done {
      let Some(Job { style, phase }) = self.jobs.remove(&batch.owner) else {
        // A job from before a font reset: its cells belong to faces that
        // are gone.
        continue;
      };
      for cell in batch.cells {
        let key = CellKey { style, phase, glyph: cell.key };
        self.pending.remove(&key);
        if let Some(atlas) = &mut self.atlas {
          match atlas.insert(cell.with_key(key)) {
            InsertOutcome::Full => {
              self.failed.insert(key);
            }
            _ => inserted += 1,
          }
        }
      }
      for glyph in batch.failed {
        let key = CellKey { style, phase, glyph };
        self.pending.remove(&key);
        self.failed.insert(key);
      }
    }
    if inserted > 0 {
      self.flush(ctx);
      self.landed = true;
    }
  }

  fn flush(&mut self, ctx: &Context) {
    if let Some(atlas) = &mut self.atlas {
      if let Err(e) = atlas.flush(ctx) {
        log::warn!("[text] atlas upload failed: {e}");
      }
    }
  }
}

// The request that makes `glyphs` of `style` at `phase`.
fn request(face: &Face, style: StyleKey, phase: u8, glyphs: Vec<u16>) -> CellRequest {
  let ppem = style.ppem();
  CellRequest {
    kind: CellKind::Mask { ppem },
    weight: face.weight_setting(style.weight),
    width: face.width_setting(style.stretch()),
    synthetic_bold: face.synthetic_bold(style.weight),
    synthetic_italic: style.italic && face.synthetic_italic(),
    phase: phase as f32 / PHASES as f32,
    darken: if style.darken { ppem * LOW_DPI_DARKEN_EM } else { 0.0 },
    glyphs,
  }
}
