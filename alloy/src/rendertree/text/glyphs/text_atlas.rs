// The text atlas: the one mask atlas every `<text>` draws from
// (okf/plans/text-own-rasterizer.md, stage 2). A cache,
// not a store: a text layer keeps pixels and never cell references, so a
// cell evicted or made at a stale display scale is only a miss at the
// layer's next rasterization, and remaking is the same path as making.
// Three sources fill it:
//
// 1. Warm-up on the worker: printable ASCII at every subpixel phase for a
//    style the moment a build first sees it untransformed (`ensure` with
//    `warm`), and whatever an app warms ahead of time (its type scale at
//    startup, under the splash). Warm jobs are small chunks, so a needed
//    job never waits long for the one in hand.
// 2. The synchronous path (`ensure`): a glyph a build needs now and the
//    atlas lacks is made on this thread, in paint order, until the frame's
//    budget is spent - time, not a count, so a slow CPU makes fewer.
// 3. Past the budget the worker makes the rest at `Needed` priority, ahead
//    of any warm-up; the layer draws without them and re-rasterizes the
//    frame they land (`land`, which every frame producer runs before it
//    decides whether the tree changed: a landing is a change to pixels
//    behind an unchanged tree, like a texture upload, so a frame that
//    would otherwise resubmit its retained display list rebuilds).
//
// What a build needs never waits on a warm-up: a glyph queued in a warm
// job is still a miss to `ensure`, made now or sent as needed, and the
// warm job's copy of it lands as a no-op.
//
// Between two `begin_frame`s no cell moves: a layer reads placements
// bucket by bucket while it builds, and a repack under it would leave the
// quads already built pointing at vacated texels. So the synchronous path
// only places what fits the atlas at its size; a cell that does not fit
// is kept and inserted at the next frame start, where the atlas may grow
// or evict, and the layer completes then like one waiting on the worker.
//
// Cells are keyed on the face, the device pixels per em, the weight, the
// style and the subpixel phase besides the glyph, so one texture serves
// every style on screen and a glyph pass binds once per layer. Frames are
// stamped on every use so a full atlas evicts what nobody drew lately.
use super::atlas::{AtlasPacker, CellPlacement, GlyphAtlas, InsertOutcome};
use super::cells::{Cell, CellKind, CellRequest, Hint, Rasterizer};
use super::fonts::{Face, FaceId, FontSet};
use super::worker::{CellJob, CellWorker, JobPriority};
use crate::gpu::{CoveragePolicy, SamplerFilter, SamplerState, SamplerWrap, MIN_ANISOTROPY};
use crate::Context;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
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
/// Glyphs per warm job: the worker takes jobs whole, so a needed job
/// waits at most this many warm cells for the one in hand (a chunk is
/// some 16 ms on the armv7 TV at a millisecond a cell).
pub const WARM_CHUNK: usize = 16;
/// Below this display scale masks may be stem-darkened by
/// `LOW_DPI_DARKEN_EM` of the em per side. Judged unnecessary by eye at 1x
/// on 2026-10-08 once the layer blended with DirectWrite's recipe, so the
/// default strength is zero; the mechanism stays behind `TextRendering`.
const LOW_DPI_SCALE: f32 = 2.0;
const LOW_DPI_DARKEN_EM: f32 = 0.0;
/// How masks below `LOW_DPI_SCALE` are hinted (`Hint`, cells.rs): at 1x
/// the x-height and cap height of a size land mid-row as often as not and
/// the glyph reads blurry beside its neighbours, so the light mode; at 2x
/// and up the rows are fine enough to leave the outline alone.
const LOW_DPI_HINT: Hint = Hint::Light;
/// The thread the text atlas's cells are made on.
const WORKER_NAME: &str = "alloy-text-cells";
const ATLAS_LABEL: &str = "text-atlas";

/// How every `<text>` is rendered: the layer's coverage-to-color policy
/// and the stem darkening its cells are made with. Settable live
/// (`PlatformContext::set_text_rendering`), so the policy can be judged by
/// eye on a running app; the defaults are what an app that says nothing
/// gets.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TextRendering {
  pub coverage: CoveragePolicy,
  /// Stem darkening in em per side at any display scale; None applies the
  /// low-DPI default (`LOW_DPI_DARKEN_EM` below `LOW_DPI_SCALE`, none
  /// above).
  pub darken: Option<f32>,
  /// The hinting mode at any display scale; None applies the low-DPI
  /// default (`LOW_DPI_HINT` below `LOW_DPI_SCALE`, unhinted above).
  pub hint: Option<Hint>,
}

impl TextRendering {
  /// The stem darkening cells are made with at `display_scale`, em per
  /// side.
  pub fn darken_em(&self, display_scale: f32) -> f32 {
    match self.darken {
      Some(em) => em,
      None if display_scale < LOW_DPI_SCALE => LOW_DPI_DARKEN_EM,
      None => 0.0,
    }
  }

  /// How cells are hinted at `display_scale`.
  pub fn hint_at(&self, display_scale: f32) -> Hint {
    match self.hint {
      Some(hint) => hint,
      None if display_scale < LOW_DPI_SCALE => LOW_DPI_HINT,
      None => Hint::Off,
    }
  }
}

/// One face at one size, weight, width, style, darkening and hinting: what
/// a run's glyphs are made as. `ppem` is the device pixels per em (the
/// run's size times the layer's scale), `stretch` the width axis
/// percentage and `darken` the stem darkening in em per side, each kept as
/// the float's bits so equal values make equal keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleKey {
  pub face: FaceId,
  ppem: u32,
  pub weight: u16,
  stretch: u32,
  pub italic: bool,
  darken: u32,
  pub hint: Hint,
}

impl StyleKey {
  pub fn new(face: FaceId, ppem: f32, weight: u16, stretch: f32, italic: bool, darken_em: f32, hint: Hint) -> Self {
    Self { face, ppem: ppem.to_bits(), weight, stretch: stretch.to_bits(), italic, darken: darken_em.to_bits(), hint }
  }

  pub fn ppem(&self) -> f32 {
    f32::from_bits(self.ppem)
  }

  pub fn stretch(&self) -> f32 {
    f32::from_bits(self.stretch)
  }

  pub fn darken_em(&self) -> f32 {
    f32::from_bits(self.darken)
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

/// A token for one job on the worker, taken from the embedder's
/// `HoldSource` when the job is submitted and dropped on the worker thread
/// once its cells are made: how the embedder counts the atlas's work in
/// flight (a headless host's settle waits for it). Opaque here; what it
/// counts into is the embedder's.
pub type WorkHold = Box<dyn Any + Send>;

/// Where a job's `WorkHold` comes from. Set per embedder session
/// (`set_hold_source`); without one the jobs run uncounted.
pub type HoldSource = Arc<dyn Fn() -> WorkHold + Send + Sync>;

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
  // The synchronous path's rasterizer: this thread's own hinter cache.
  rasterizer: Rasterizer,
  jobs: HashMap<u64, Job>,
  next_job: u64,
  // Cells on the worker or deferred to the next frame start, by the
  // priority they went at: a cell pending at `Needed` is not asked for
  // again, one pending at `Warm` still counts as a miss to a build (see
  // the header). A warm-up skips both.
  pending: HashMap<CellKey, JobPriority>,
  // Cells made on the synchronous path that did not fit the atlas at its
  // size: inserted at the next frame start, when the atlas may grow (see
  // the header).
  deferred: Vec<Cell<CellKey>>,
  // What a job's hold is taken from. A job's work is in flight from its
  // submission until the worker has made its cells, and is counted there,
  // on the worker thread: never at the landing, which only a frame does,
  // and a frame only runs once nothing is in flight.
  hold_source: Option<HoldSource>,
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
  // The platform's frame request latch: what a landing job sets from the
  // worker thread, with the loop's wake, so the frame that completes the
  // layers runs; and what a warm-up asked for ahead of use sets, so the
  // frame that submits it runs.
  frame_request: Arc<AtomicBool>,
  wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl TextAtlas {
  /// An empty atlas over `frame_request`, the platform's latch.
  pub fn new(frame_request: Arc<AtomicBool>) -> Self {
    Self {
      atlas: None,
      worker: None,
      worker_failed: false,
      rasterizer: Rasterizer::default(),
      jobs: HashMap::new(),
      next_job: 1,
      pending: HashMap::new(),
      deferred: Vec::new(),
      hold_source: None,
      failed: HashSet::new(),
      warmed: HashSet::new(),
      warm_requests: Vec::new(),
      frame: 0,
      deadline: None,
      landed: false,
      stale: false,
      frame_request,
      wake: None,
    }
  }

  /// Where the jobs' holds come from (see `WorkHold`). An embedder sets it
  /// when its session starts and again for the next one: a job submitted
  /// afterwards is counted into the new source, one already on the worker
  /// releases into the old.
  pub fn set_hold_source(&mut self, source: HoldSource) {
    self.hold_source = Some(source);
  }

  /// Start a frame's build (after its `land`): open this frame's
  /// synchronous budget, and queue the warm-ups asked for since the last
  /// frame at `display_scale`, darkened by `darken_em` (em per side),
  /// hinted as `hint` says.
  pub fn begin_frame(&mut self, ctx: &Context, fonts: &FontSet, display_scale: f32, darken_em: f32, hint: Hint) {
    if self.wake.is_none() {
      self.wake = ctx.frame_wake();
    }
    self.frame += 1;
    if let Some(atlas) = &mut self.atlas {
      atlas.packer_mut().begin_frame(self.frame);
    }
    self.deadline = Some(Instant::now() + SYNC_CELL_BUDGET);
    for request in std::mem::take(&mut self.warm_requests) {
      let key = StyleKey::new(
        request.face,
        request.size * display_scale,
        request.weight,
        request.stretch,
        request.italic,
        darken_em,
        hint,
      );
      if !self.warmed.contains(&key) {
        self.warm(fonts, key);
      }
    }
  }

  /// Ask for a style's printable ASCII ahead of its use; made at the next
  /// frame start on the worker, and that frame is requested. A style
  /// already warmed costs nothing.
  pub fn request_warm(&mut self, request: WarmRequest) {
    if !self.warm_requests.contains(&request) {
      self.warm_requests.push(request);
      self.frame_request.store(true, Ordering::Relaxed);
    }
  }

  /// Whether this frame's `land` put cells in the atlas: every incomplete
  /// layer then re-rasterizes.
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

  /// Cells not in the atlas yet: on the worker, or deferred to the next
  /// frame start.
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
  /// past it (a None in the result, filled in at a later frame). With
  /// `warm`, also warms the style's ASCII on first sight: a layer passes
  /// it for an untransformed text, whose style every other text of that
  /// size shares, and not for one under a scaling transform, whose ppem
  /// is its own. Returns how many came back None.
  #[allow(clippy::too_many_arguments)]
  pub fn ensure(
    &mut self,
    ctx: &Context,
    fonts: &FontSet,
    style: StyleKey,
    phase: u8,
    glyphs: &[u16],
    warm: bool,
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
          let needed = self.pending.get(&key) == Some(&JobPriority::Needed);
          if !self.failed.contains(&key) && !needed && !misses.contains(&glyph) {
            misses.push(glyph);
          }
          out.push(None);
        }
      }
    }
    if misses.is_empty() {
      self.warm_once(fonts, style, warm);
      return out.iter().filter(|p| p.is_none()).count();
    }
    // Within the budget the misses are made here and now; past it they go
    // to the worker ahead of every warm-up.
    let in_budget = self.deadline.is_some_and(|deadline| Instant::now() < deadline);
    if in_budget {
      let request = request(face, style, phase, misses.clone());
      let made = self.rasterizer.rasterize(face.bytes(), face.styles(), &request).unwrap_or_default();
      let mut inserted = 0;
      for cell in made {
        let key = CellKey { style, phase, glyph: cell.key };
        let atlas = self.atlas.as_mut().expect("opened above");
        match atlas.insert_in_place(cell.with_key(key)) {
          Ok(()) => inserted += 1,
          // No room at this size: the next frame start grows the atlas
          // around it, and that frame is requested.
          Err(cell) => {
            self.pending.insert(key, JobPriority::Needed);
            self.deferred.push(cell);
            self.frame_request.store(true, Ordering::Relaxed);
          }
        }
      }
      for &glyph in &misses {
        let key = CellKey { style, phase, glyph };
        let atlas = self.atlas.as_ref().expect("opened above");
        if atlas.packer().placement(key).is_none() && !self.pending.contains_key(&key) {
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
    // After the build's own glyphs, so the warm-up skips what they made
    // or queued.
    self.warm_once(fonts, style, warm);
    out.iter().filter(|p| p.is_none()).count()
  }

  // The first-sight warm-up of `ensure`: once per style, and only when
  // the build asks for it.
  fn warm_once(&mut self, fonts: &FontSet, style: StyleKey, warm: bool) {
    if warm && !self.warmed.contains(&style) {
      self.warm(fonts, style);
    }
  }

  /// Make `style`'s printable ASCII at every phase on the worker, ahead of
  /// its use, in jobs of `WARM_CHUNK` glyphs. A style already warmed is a
  /// no-op.
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
          !self.pending.contains_key(&key)
            && !self.failed.contains(&key)
            && self.atlas.as_ref().is_none_or(|a| a.packer().placement(key).is_none())
        })
        .collect();
      for chunk in missing.chunks(WARM_CHUNK) {
        self.submit(face, style, phase, chunk.to_vec(), JobPriority::Warm);
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
      self.deferred.clear();
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
    let hold = self.hold_source.as_ref().map(|source| source());
    let done: Box<dyn FnOnce() + Send> = Box::new(move || {
      // The cells are queued for the landing by now: the frame that lands
      // them is requested first, then the job stops counting as in flight,
      // so whoever that releases finds the frame demanded.
      latch.store(true, Ordering::Relaxed);
      if let Some(wake) = wake {
        wake();
      }
      drop(hold);
    });
    let job = CellJob {
      owner,
      bytes: face.bytes().clone(),
      styles: face.styles().clone(),
      request,
      priority,
      done: Some(done),
    };
    // A job the worker refuses is dropped unsent, its closure and its hold
    // with it.
    if worker.submit(job) {
      self.jobs.insert(owner, Job { style, phase });
      for glyph in glyphs {
        self.pending.insert(CellKey { style, phase, glyph }, priority);
      }
    }
  }

  /// Land the cells deferred for room and every finished job in the atlas,
  /// growing it as needed (nothing holds a placement now), and upload.
  /// Once per frame, before the frame producer decides whether anything
  /// changed (see the header); `landed` says whether cells came in.
  pub fn land(&mut self, ctx: &Context) {
    self.landed = false;
    let deferred = std::mem::take(&mut self.deferred);
    let done = self.worker.as_ref().map(|w| w.drain()).unwrap_or_default();
    if deferred.is_empty() && done.is_empty() {
      return;
    }
    let mut inserted = 0;
    for cell in deferred {
      let key = cell.key;
      self.pending.remove(&key);
      if let Some(atlas) = &mut self.atlas {
        match atlas.insert(cell) {
          InsertOutcome::Full => {
            self.failed.insert(key);
          }
          _ => inserted += 1,
        }
      }
    }
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
    darken: ppem * style.darken_em(),
    hint: style.hint,
    glyphs,
  }
}
