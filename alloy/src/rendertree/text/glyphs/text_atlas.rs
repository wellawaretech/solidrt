// The text atlas: the one mask atlas every `<text>` draws from
// (okf/done/text-own-rasterizer.md, stage 2; the completion rule from
// okf/done/text-complete-frames.md). A cache, not a store: a text layer
// keeps pixels and never cell references, so a cell evicted or made at a
// stale display scale is only a miss at the layer's next rasterization,
// and remaking is the same path as making.
//
// The rule: a frame that draws text has every glyph it draws. A build
// asks `ensure` for the cells of each run, and a cell the atlas lacks is
// made on this thread, now, however many there are; a cold style makes
// its frame late, as in every renderer, and never incomplete. Pixels are
// thus a function of the tree alone: a headless render is deterministic
// and a snapshot is complete by construction.
//
// Two sources fill the atlas:
//
// 1. The build (`ensure`): what a layer needs and lacks, in paint order.
// 2. Warm-up on the worker: printable ASCII at every subpixel phase for a
//    style the moment a build first sees it untransformed (`ensure` with
//    `warm`), and whatever an app warms ahead of time (its type scale at
//    startup, under the splash, with the strings it knows it will draw),
//    landed at the next frame start (`land`).
//    An optimization and nothing more: a build that needs a cell still
//    queued in a warm job makes it itself, and the job's copy lands as a
//    no-op, so nothing visible waits on the worker and a warm job holds
//    nothing. The stall it saves is the whole first paint of a cold style
//    (about a millisecond a cell on an armv7 TV).
//
// The packer exists from construction; only its texture needs the GPU
// context and is created at the first `flush`, so cells land whether or
// not any text has drawn yet (a `warmText` at startup) and the atlas is
// checkable without a GPU. A layer asks for all of its cells before it
// reads any placement, so a growth or an eviction under one of its
// buckets moves nothing it has used; layers built earlier in the frame
// are finished, their pixels in their own textures, before the atlas
// moves beneath them.
//
// Cells are keyed on the face, the device pixels per em, the weight, the
// style and the subpixel phase besides the glyph, so one texture serves
// every style on screen and a glyph pass binds once per layer. Frames are
// stamped on every use so a full atlas evicts what nobody drew lately; an
// eviction also forgets that the style was warmed, so its next first sight
// warms it again.
use super::atlas::{create_texture, flush_texture, AtlasPacker, CellPlacement, InsertOutcome};
use super::cells::{Cell, CellKind, CellRequest, Hint, Rasterizer};
use super::fonts::{Face, FaceId, FontSet};
use super::shape::{Fallback, ShapeStyle, ShapedGlyphs};
use super::worker::{CellJob, CellWorker, JobPriority};
use crate::gpu::{CoveragePolicy, SamplerFilter, SamplerState, SamplerWrap, MIN_ANISOTROPY};
use crate::Context;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Subpixel phases per glyph along x: a cell per third of a pixel, the
/// spike's choice (Skia positions at quarters).
pub const PHASES: u8 = 3;
/// Frames a cell may go unused before a full atlas evicts it: two seconds
/// at 60 Hz, so a screen that comes right back keeps its cells.
const EVICT_AFTER_FRAMES: u64 = 120;
/// The growth cap before a frame has set the device's (`set_texture_cap`):
/// the texture side GLES 3.0 guarantees, so nothing packed ahead of the
/// first frame can exceed what the GPU takes.
const PROVISIONAL_MAX_SIDE: u32 = 2048;
/// The code points a warm-up makes for a style: printable ASCII.
const WARM_FIRST: u32 = 0x20;
const WARM_LAST: u32 = 0x7e;
/// Glyphs per warm job: the worker takes jobs whole and an engine teardown
/// joins the one in hand, so a chunk bounds that wait to some 16 ms on the
/// armv7 TV at a millisecond a cell; and cells land per chunk, so a build
/// that comes mid-warm finds the early ones.
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
/// The thread the text atlas's warm-up cells are made on.
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
/// frame, when the display scale is known, and optionally a string it will
/// draw in it, whose glyphs are warmed beside the style's ASCII.
#[derive(Clone, Debug, PartialEq)]
pub struct WarmRequest {
  pub face: FaceId,
  pub size: f32,
  pub weight: u16,
  pub stretch: f32,
  pub italic: bool,
  pub text: Option<String>,
}

// A job on the worker, by its owner id: which cells it makes.
struct Job {
  style: StyleKey,
  phase: u8,
}

pub struct TextAtlas {
  packer: AtlasPacker<CellKey>,
  // The packer's texture, created at the first `flush`; None until then,
  // and for good once the GPU refused it (logged once).
  texture: Option<u64>,
  texture_failed: bool,
  worker: Option<CellWorker>,
  worker_failed: bool,
  // The build's rasterizer: this thread's own hinter cache.
  rasterizer: Rasterizer,
  jobs: HashMap<u64, Job>,
  next_job: u64,
  // Cells queued in a warm job and not landed yet: a warm-up does not
  // queue them again; a build still makes them when it needs them.
  pending: HashSet<CellKey>,
  // Cells the rasterizer could not make, or the atlas could not place at
  // its cap: not asked for again until a font reset.
  failed: HashSet<CellKey>,
  // Whether the cap was reported; once is enough for a read of the log.
  full_logged: bool,
  warmed: HashSet<StyleKey>,
  // Styles asked for ahead of use, warmed at the next frame start.
  warm_requests: Vec<WarmRequest>,
  frame: u64,
  // The platform's frame request latch: what a warm-up asked for ahead of
  // use sets, so the frame that submits it runs.
  frame_request: Arc<AtomicBool>,
}

impl TextAtlas {
  /// An empty atlas over `frame_request`, the platform's latch.
  pub fn new(frame_request: Arc<AtomicBool>) -> Self {
    let mut packer =
      AtlasPacker::new(CellKind::Mask { ppem: 0.0 }, PROVISIONAL_MAX_SIDE).with_eviction(EVICT_AFTER_FRAMES);
    packer.begin_frame(0);
    Self {
      packer,
      texture: None,
      texture_failed: false,
      worker: None,
      worker_failed: false,
      rasterizer: Rasterizer::default(),
      jobs: HashMap::new(),
      next_job: 1,
      pending: HashSet::new(),
      failed: HashSet::new(),
      full_logged: false,
      warmed: HashSet::new(),
      warm_requests: Vec::new(),
      frame: 0,
      frame_request,
    }
  }

  /// The growth cap: the device's texture size limit, set by the frame
  /// producer once there is a GPU context to ask (every frame start is
  /// fine, it is a store).
  pub fn set_texture_cap(&mut self, max_side: u32) {
    self.packer.set_max_side(max_side);
  }

  /// Start a frame's build (after its `land`): stamp the frame, and queue
  /// the warm-ups asked for since the last frame at `display_scale`,
  /// darkened by `darken_em` (em per side), hinted as `hint` says.
  pub fn begin_frame(&mut self, fonts: &FontSet, display_scale: f32, darken_em: f32, hint: Hint) {
    self.frame += 1;
    self.packer.begin_frame(self.frame);
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
      if let Some(text) = &request.text {
        self.warm_text(fonts, &request, key, text);
      }
    }
  }

  /// Ask for a style's printable ASCII, and the glyphs of its `text` if it
  /// names one, ahead of their use; made at the next frame start on the
  /// worker, and that frame is requested. A style already warmed costs
  /// nothing; a text's cells already in the atlas cost nothing.
  pub fn request_warm(&mut self, request: WarmRequest) {
    if !self.warm_requests.contains(&request) {
      self.warm_requests.push(request);
      self.frame_request.store(true, Ordering::Relaxed);
    }
  }

  /// Whether `style`'s warm-up was queued and its cells not evicted since.
  pub fn is_warmed(&self, style: StyleKey) -> bool {
    self.warmed.contains(&style)
  }

  /// The frames begun so far: what a text layer stamps its last use with.
  pub fn frame(&self) -> u64 {
    self.frame
  }

  /// The atlas texture, once a layer flushed it.
  pub fn texture(&self) -> Option<u64> {
    self.texture
  }

  /// The atlas's current size in texels.
  pub fn size(&self) -> (u32, u32) {
    self.packer.size()
  }

  /// Cells queued on the worker and not landed yet.
  pub fn queued_cells(&self) -> usize {
    self.pending.len()
  }

  /// The registered fonts changed (an app switch): every cell is void. The
  /// texture is kept, cleared at the next flush; a job still on the worker
  /// lands as nothing.
  pub fn invalidate_fonts(&mut self) {
    self.packer.clear();
    self.packer.begin_frame(self.frame);
    self.pending.clear();
    self.jobs.clear();
    self.warmed.clear();
    self.failed.clear();
  }

  /// Have the cells of `glyphs` for `style` at `phase` in the atlas,
  /// making what it lacks now, on this thread, growing or evicting as
  /// needed; `placement` reads them afterwards, once every bucket of the
  /// layer was ensured. With `warm`, also warms the style's ASCII on first
  /// sight: a layer passes it for an untransformed text, whose style every
  /// other text of that size shares, and not for one under a scaling
  /// transform, whose ppem is its own. Returns how many of `glyphs` have
  /// no cell even so (the rasterizer made nothing, or the atlas is at its
  /// cap): drawn as nothing at their advance.
  pub fn ensure(&mut self, fonts: &FontSet, style: StyleKey, phase: u8, glyphs: &[u16], warm: bool) -> usize {
    let Some(face) = fonts.face(style.face) else {
      return glyphs.len();
    };
    let mut misses: Vec<u16> = Vec::new();
    for &glyph in glyphs {
      let key = CellKey { style, phase, glyph };
      if self.packer.touch(key) || self.failed.contains(&key) || misses.contains(&glyph) {
        continue;
      }
      misses.push(glyph);
    }
    if !misses.is_empty() {
      let request = request(face, style, phase, misses.clone());
      let made = self.rasterizer.rasterize(face.bytes(), face.styles(), &request).unwrap_or_default();
      for cell in made {
        let key = CellKey { style, phase, glyph: cell.key };
        self.insert(cell.with_key(key));
      }
      for &glyph in &misses {
        let key = CellKey { style, phase, glyph };
        if self.packer.placement(key).is_none() {
          self.failed.insert(key);
        }
      }
    }
    self.warm_once(fonts, style, warm);
    glyphs.iter().filter(|&&glyph| self.packer.placement(CellKey { style, phase, glyph }).is_none()).count()
  }

  /// The placement of a cell `ensure` had in the atlas, counted as used
  /// this frame. None for a glyph `ensure` reported as missing.
  pub fn placement(&mut self, style: StyleKey, phase: u8, glyph: u16) -> Option<CellPlacement> {
    let key = CellKey { style, phase, glyph };
    self.packer.touch(key);
    self.packer.placement(key)
  }

  /// Put the atlas on the GPU as it stands: create the texture at the first
  /// call, upload what changed since the last. The texture id a glyph pass
  /// samples, or None when the GPU refused the texture (logged once).
  pub fn flush(&mut self, ctx: &Context) -> Option<u64> {
    if self.texture.is_none() && !self.texture_failed {
      let sampler = SamplerState {
        filter: SamplerFilter::Linear,
        wrap: SamplerWrap::Clamp,
        mipmap: false,
        anisotropy: MIN_ANISOTROPY,
      };
      match create_texture(ctx, &mut self.packer, sampler, ATLAS_LABEL) {
        Ok(texture) => self.texture = Some(texture),
        Err(e) => {
          log::warn!("[text] the text atlas texture could not be created: {e}");
          self.texture_failed = true;
        }
      }
    }
    let texture = self.texture?;
    if let Err(e) = flush_texture(ctx, texture, &mut self.packer) {
      log::warn!("[text] atlas upload failed: {e}");
    }
    Some(texture)
  }

  /// Land every finished warm job in the atlas, growing it as needed. Once
  /// per frame, ahead of the frame's builds, so a build finds what the
  /// worker made instead of making it again.
  pub fn land(&mut self) {
    let done = self.worker.as_ref().map(|w| w.drain()).unwrap_or_default();
    for batch in done {
      let Some(Job { style, phase }) = self.jobs.remove(&batch.owner) else {
        // A job from before a font reset: its cells belong to faces that
        // are gone.
        continue;
      };
      for cell in batch.cells {
        let key = CellKey { style, phase, glyph: cell.key };
        self.pending.remove(&key);
        self.insert(cell.with_key(key));
      }
      for glyph in batch.failed {
        let key = CellKey { style, phase, glyph };
        self.pending.remove(&key);
        self.failed.insert(key);
      }
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
    self.warm_glyphs(face, style, glyphs);
  }

  // Make the cells of `text` shaped in the request's style ahead of a text
  // drawing it: the glyphs the shaper picks (a ligature, a curly quote, a
  // letter beyond ASCII), each keyed on the face that covers it, since a
  // cluster the primary lacks draws from a fallback face's cells. `style`
  // is the primary's key; a fallback's is the same key on its face.
  fn warm_text(&mut self, fonts: &FontSet, request: &WarmRequest, style: StyleKey, text: &str) {
    let shaping = ShapeStyle {
      weight: request.weight,
      stretch: request.stretch,
      size: request.size,
      line_height: 0.0,
      letter_spacing: 0.0,
    };
    let Some(shaped) = ShapedGlyphs::shape(fonts, request.face, text, &shaping, Fallback::Registered) else { return };
    let mut by_face: Vec<(FaceId, Vec<u16>)> = Vec::new();
    for g in &shaped.glyphs {
      match by_face.iter_mut().find(|(face, _)| *face == g.face) {
        Some((_, ids)) => {
          if !ids.contains(&g.id) {
            ids.push(g.id);
          }
        }
        None => by_face.push((g.face, vec![g.id])),
      }
    }
    for (face_id, ids) in by_face {
      let Some(face) = fonts.face(face_id) else { continue };
      self.warm_glyphs(face, StyleKey { face: face_id, ..style }, ids);
    }
  }

  // Make `glyphs` of `style` at every phase on the worker: the ones the
  // atlas lacks, has not queued and has not failed, in chunks.
  fn warm_glyphs(&mut self, face: &Face, style: StyleKey, glyphs: Vec<u16>) {
    for phase in 0..PHASES {
      let missing: Vec<u16> = glyphs
        .iter()
        .copied()
        .filter(|&glyph| {
          let key = CellKey { style, phase, glyph };
          !self.pending.contains(&key) && !self.failed.contains(&key) && self.packer.placement(key).is_none()
        })
        .collect();
      for chunk in missing.chunks(WARM_CHUNK) {
        self.submit(face, style, phase, chunk.to_vec());
      }
    }
  }

  // The first-sight warm-up of `ensure`: once per style, and only when
  // the build asks for it.
  fn warm_once(&mut self, fonts: &FontSet, style: StyleKey, warm: bool) {
    if warm && !self.warmed.contains(&style) {
      self.warm(fonts, style);
    }
  }

  // Add a cell, growing or evicting as needed. A cell the atlas cannot
  // place at its cap is failed (not asked for again) and the cap reported
  // once. A key already present is a touch.
  fn insert(&mut self, cell: Cell<CellKey>) {
    let key = cell.key;
    if self.packer.insert(cell) == InsertOutcome::Full {
      self.failed.insert(key);
      if !self.full_logged {
        let (width, height) = self.packer.size();
        log::warn!("[text] the text atlas is full at {width}x{height} texels: glyphs nobody drew lately were evicted and the rest do not fit, so some glyphs will be missing");
        self.full_logged = true;
      }
    }
    self.forget_evicted();
  }

  // A style whose cells an eviction took is no longer warmed: its next
  // first sight warms it again.
  fn forget_evicted(&mut self) {
    for key in self.packer.take_evicted() {
      self.warmed.remove(&key.style);
    }
  }

  fn submit(&mut self, face: &Face, style: StyleKey, phase: u8, glyphs: Vec<u16>) {
    if self.worker.is_none() && !self.worker_failed {
      match CellWorker::spawn(WORKER_NAME) {
        Ok(worker) => self.worker = Some(worker),
        Err(e) => {
          log::warn!("[text] the glyph worker did not start: {e}; cells are made at first use only");
          self.worker_failed = true;
        }
      }
    }
    let Some(worker) = &self.worker else { return };
    let owner = self.next_job;
    self.next_job += 1;
    let job = CellJob {
      owner,
      bytes: face.bytes().clone(),
      styles: face.styles().clone(),
      request: request(face, style, phase, glyphs.clone()),
      priority: JobPriority::Warm,
      done: None,
    };
    // A job the worker refuses is dropped unsent.
    if worker.submit(job) {
      self.jobs.insert(owner, Job { style, phase });
      for glyph in glyphs {
        self.pending.insert(CellKey { style, phase, glyph });
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
