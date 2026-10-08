//! JS bindings for the glyph engine: `flux:font`, a thin marshaling layer
//! over `alloy::rendertree::text::glyphs`. A font handle is one registered
//! face at one weight and style with an atlas of cells of one kind; the app
//! prepares text over it (the engine shaper, every unit carrying its
//! glyphs), asks for the glyph cells it needs and samples the atlas texture
//! itself. Cells are made on the engine's worker thread and land in the
//! atlas at `tick` (the per-frame hook, see frame::advance), which also
//! settles the `requestGlyphs` promises whose glyphs have all arrived; a
//! request is work in flight, so `settle` waits for it.
//!
//! Every font this engine created is released with it (the plugin owns
//! what it opens): its atlas texture goes, the worker thread is joined.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use alloy::rendertree::text::glyphs::{
  weight_value, AtlasPacker, CellJob, CellKind, CellPlacement, CellRequest, CellWorker, GlyphAtlas, InsertOutcome,
  JobPriority,
};
use alloy::rendertree::text::{prepare_units, Fallback, RunStyle};
use alloy::rendertree::Text;
use alloy::{SamplerFilter, SamplerState, SamplerWrap, MIN_ANISOTROPY};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promise;
use rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object, Persistent};

use crate::plugins::marshal::OptArg;

/// Texels per em of a distance-field atlas when the app names none: what
/// Godot's MSDF import samples at, sharp through the zoom range labels see.
const DEFAULT_MSDF_SIZE: f32 = 48.0;
/// The distance range of a distance-field atlas in texels when the app
/// names none (Godot's pixel range): room for an outline a few pixels wide.
const DEFAULT_MSDF_RANGE: f32 = 8.0;
/// The thread the engine's cells are made on.
const WORKER_NAME: &str = "flux-glyph-cells";

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> rquickjs::Error {
  rquickjs::Exception::throw_message(ctx, msg)
}

/// One created font: the face and style it shapes with, its atlas, and
/// which glyphs are in flight or could not be made.
struct FontEntry {
  face: usize,
  style: RunStyle,
  kind: CellKind,
  atlas: GlyphAtlas<u16>,
  /// Glyphs handed to the worker or already placed.
  requested: HashSet<u16>,
  /// Glyphs the worker could not rasterize, or the full atlas refused:
  /// a request for them settles with null instead of waiting forever.
  failed: HashSet<u16>,
}

// A `requestGlyphs` promise waiting for its glyphs. The work in flight is
// the job on the worker: its hold travels with the job and ends when the
// cells are made, which also latches a frame request and wakes the loop,
// so the tick that lands the cells and settles this runs next - in the
// client, and in a headless host that steps a frame only once nothing is
// in flight and one is demanded.
struct PendingRequest {
  font: u64,
  glyphs: Vec<u16>,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

struct Inner {
  gui: Rc<super::Gui>,
  /// None when the thread could not start; requests then reject.
  worker: Option<CellWorker>,
  fonts: RefCell<HashMap<u64, FontEntry>>,
  next_id: std::cell::Cell<u64>,
  pending: RefCell<Vec<PendingRequest>>,
}

impl Drop for Inner {
  // Engine teardown: the app's handles died with it, so nothing else will
  // ever destroy its fonts. The worker joins when the field drops.
  fn drop(&mut self) {
    for entry in self.fonts.get_mut().values_mut() {
      entry.atlas.destroy(&self.gui.alloy);
    }
  }
}

#[derive(Clone, JsLifetime)]
struct FontPluginState(#[qjs(skip_trace)] Rc<Inner>);

fn state(ctx: &Ctx<'_>) -> Rc<Inner> {
  ctx.userdata::<FontPluginState>().expect("font state userdata").0.clone()
}

/// Store the font plugin state in userdata. Runs at engine init (before any
/// module import) so `FontModule::evaluate` and the per-frame `tick` can
/// read it. The `flux:font` module surface is registered separately via
/// `module`.
pub(crate) fn store_state(ctx: &Ctx<'_>) {
  let worker = match CellWorker::spawn(WORKER_NAME) {
    Ok(worker) => Some(worker),
    Err(e) => {
      log::warn!("[font] glyph worker did not start: {e}; glyph requests will reject");
      None
    }
  };
  ctx
    .store_userdata(FontPluginState(Rc::new(Inner {
      gui: super::gui(ctx),
      worker,
      fonts: RefCell::new(HashMap::new()),
      next_id: std::cell::Cell::new(1),
      pending: RefCell::new(Vec::new()),
    })))
    .expect("store font state");
}

/// The `flux:font` module.
pub struct FontModule;

impl ModuleDef for FontModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("createFont")?;
    decl.declare("destroyFont")?;
    decl.declare("fontAtlas")?;
    decl.declare("prepareText")?;
    decl.declare("requestGlyphs")?;
    decl.declare("glyphCells")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("createFont", Function::new(ctx.clone(), create_font)?)?;
    exports.export("destroyFont", Function::new(ctx.clone(), destroy_font)?)?;
    exports.export("fontAtlas", Function::new(ctx.clone(), font_atlas)?)?;
    exports.export("prepareText", Function::new(ctx.clone(), prepare_text)?)?;
    exports.export("requestGlyphs", Function::new(ctx.clone(), request_glyphs)?)?;
    exports.export("glyphCells", Function::new(ctx.clone(), glyph_cells)?)?;
    Ok(())
  }
}

// The face options (fontFamily, fontSize, fontWeight, fontStyle,
// fontStretch, letterSpacing, lineHeight) through the JSX decoders onto a
// Text, then its run style:
// one parser for the whole runtime's font vocabulary.
fn run_style<'js>(ctx: &Ctx<'js>, options: Option<&Object<'js>>) -> rquickjs::Result<RunStyle> {
  let mut node = Text::default();
  if let Some(opts) = options {
    super::tree::apply_font_options(ctx, &mut node, opts)?;
  }
  Ok(node.run_style())
}

fn create_font<'js>(ctx: Ctx<'js>, face: OptArg<Object<'js>>, options: OptArg<Object<'js>>) -> rquickjs::Result<u64> {
  let style = run_style(&ctx, face.0.as_ref())?;
  let mut cells = "msdf".to_string();
  let mut size: Option<f32> = None;
  let mut range: Option<f32> = None;
  let mut mipmap: Option<bool> = None;
  let mut label: Option<String> = None;
  if let Some(opts) = options.0 {
    if let Some(value) = opts.get::<_, Option<String>>("cells")? {
      cells = value;
    }
    size = opts.get("size")?;
    range = opts.get("range")?;
    mipmap = opts.get("mipmap")?;
    label = opts.get("label")?;
  }
  let kind = match cells.as_str() {
    // A mask atlas is drawn 1:1, so its cells are the face's own size.
    "mask" => CellKind::Mask { ppem: size.unwrap_or(style.font_size) },
    "msdf" => CellKind::Msdf { ppem: size.unwrap_or(DEFAULT_MSDF_SIZE), range: range.unwrap_or(DEFAULT_MSDF_RANGE) },
    other => return Err(throw_str(&ctx, &format!("createFont: unknown cells kind '{other}' (mask or msdf)"))),
  };
  if !(kind.ppem() > 0.0 && kind.ppem().is_finite()) {
    return Err(throw_str(&ctx, &format!("createFont: size must be a positive number, got {}", kind.ppem())));
  }
  let st = state(&ctx);
  let face = {
    let fonts = st.gui.platform.glyphs();
    fonts.resolve(&style.font_family).ok_or_else(|| throw_str(&ctx, "createFont: no fonts are registered"))?
  };
  let sampler = SamplerState {
    filter: SamplerFilter::Linear,
    wrap: SamplerWrap::Clamp,
    // A distance field is sampled at every zoom, so it minifies through a
    // chain by default; a mask is drawn 1:1.
    mipmap: mipmap.unwrap_or(matches!(kind, CellKind::Msdf { .. })),
    anisotropy: MIN_ANISOTROPY,
  };
  let max_side = st.gui.alloy.gpu_limits().max_texture_size;
  let id = st.next_id.get();
  let label = label.unwrap_or_else(|| format!("font-{id}"));
  let atlas = GlyphAtlas::new(&st.gui.alloy, AtlasPacker::new(kind, max_side), sampler, &label)
    .map_err(|e| throw_str(&ctx, &format!("createFont: {e}")))?;
  st.next_id.set(id + 1);
  st.fonts
    .borrow_mut()
    .insert(id, FontEntry { face, style, kind, atlas, requested: HashSet::new(), failed: HashSet::new() });
  Ok(id)
}

fn destroy_font(ctx: Ctx<'_>, font: u64) {
  let st = state(&ctx);
  let removed = st.fonts.borrow_mut().remove(&font);
  if let Some(mut entry) = removed {
    entry.atlas.destroy(&st.gui.alloy);
  }
  // Requests on it settle at the next tick (rejected: the font is gone).
}

fn font_atlas(ctx: Ctx<'_>, font: u64) -> rquickjs::Result<Object<'_>> {
  let st = state(&ctx);
  let fonts = st.fonts.borrow();
  let entry = fonts.get(&font).ok_or_else(|| throw_str(&ctx, &format!("fontAtlas: unknown font {font}")))?;
  let (width, height) = entry.atlas.packer().size();
  let obj = Object::new(ctx.clone())?;
  obj.set("texture", entry.atlas.texture())?;
  obj.set("width", width)?;
  obj.set("height", height)?;
  obj.set("fontSize", entry.style.font_size)?;
  match entry.kind {
    CellKind::Mask { ppem } => {
      obj.set("cells", "mask")?;
      obj.set("size", ppem)?;
      obj.set("range", 0.0f32)?;
    }
    CellKind::Msdf { ppem, range } => {
      obj.set("cells", "msdf")?;
      obj.set("size", ppem)?;
      obj.set("range", range)?;
    }
  }
  Ok(obj)
}

fn prepare_text<'js>(
  ctx: Ctx<'js>,
  font: u64,
  text: String,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Object<'js>> {
  let st = state(&ctx);
  let mut style = {
    let fonts = st.fonts.borrow();
    let entry = fonts.get(&font).ok_or_else(|| throw_str(&ctx, &format!("prepareText: unknown font {font}")))?;
    entry.style.clone()
  };
  let mut carets = false;
  if let Some(opts) = options.0 {
    if let Some(size) = opts.get::<_, Option<f32>>("fontSize")? {
      if !(size > 0.0 && size.is_finite()) {
        return Err(throw_str(&ctx, &format!("prepareText: fontSize must be a positive number, got {size}")));
      }
      style.font_size = size;
    }
    if let Some(line_height) = opts.get::<_, Option<f32>>("lineHeight")? {
      if !(line_height >= 0.0 && line_height.is_finite()) {
        return Err(throw_str(
          &ctx,
          &format!("prepareText: lineHeight must be a non-negative multiplier, got {line_height}"),
        ));
      }
      style.line_height = line_height;
    }
    if let Some(letter_spacing) = opts.get::<_, Option<f32>>("letterSpacing")? {
      if !letter_spacing.is_finite() {
        return Err(throw_str(&ctx, &format!("prepareText: letterSpacing must be a finite number, got {letter_spacing}")));
      }
      style.letter_spacing = letter_spacing;
    }
    carets = opts.get::<_, bool>("carets").unwrap_or(false);
  }
  // One face per handle, so a character it lacks is its notdef: this
  // atlas holds no other face's cells.
  let units = prepare_units(&st.gui.platform, &text, &style, &[], carets, Fallback::None);
  super::tree::prepared_to_js(&ctx, text, units, true)
}

// Glyph ids from JS: non-negative integers within the font's id range.
fn glyph_ids(ctx: &Ctx<'_>, what: &str, glyphs: Vec<f64>) -> rquickjs::Result<Vec<u16>> {
  glyphs
    .into_iter()
    .map(|g| {
      if g.fract() == 0.0 && g >= 0.0 && g <= u16::MAX as f64 {
        Ok(g as u16)
      } else {
        Err(throw_str(ctx, &format!("{what}: glyph ids are integers in 0..65535, got {g}")))
      }
    })
    .collect()
}

fn request_glyphs<'js>(ctx: Ctx<'js>, font: u64, glyphs: Vec<f64>) -> rquickjs::Result<Promise<'js>> {
  let glyphs = glyph_ids(&ctx, "requestGlyphs", glyphs)?;
  let st = state(&ctx);
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let submitted = {
    let mut fonts = st.fonts.borrow_mut();
    let entry = fonts.get_mut(&font).ok_or_else(|| throw_str(&ctx, &format!("requestGlyphs: unknown font {font}")))?;
    let missing: Vec<u16> =
      glyphs.iter().copied().filter(|g| !entry.requested.contains(g) && !entry.failed.contains(g)).collect();
    if missing.is_empty() {
      true
    } else {
      let face = st.gui.platform.glyphs();
      let face = face
        .face(entry.face)
        .ok_or_else(|| throw_str(&ctx, "requestGlyphs: the font's face is no longer registered"))?;
      let weight = weight_value(entry.style.font_weight);
      let request = CellRequest {
        kind: entry.kind,
        weight: face.weight_setting(weight),
        width: face.width_setting(entry.style.font_stretch),
        synthetic_bold: face.synthetic_bold(weight),
        synthetic_italic: entry.style.font_style == alloy::impellers::FontStyle::Italic && face.synthetic_italic(),
        // A handle's cells are placed by their sampler (msdf) or drawn at
        // whole pixels (mask): one phase, no darkening policy.
        phase: 0.0,
        darken: 0.0,
        hint: false,
        glyphs: missing.clone(),
      };
      // The hold ends on the worker thread when the cells are made; the
      // latch and the wake bring the frame that lands them, and go first,
      // so whoever the hold's end releases finds the frame demanded.
      let hold = crate::pending::PendingOps::of(&ctx).in_flight("glyph cells");
      let latch = st.gui.platform.frame_request_handle();
      let wake = st.gui.alloy.frame_wake();
      let done: Box<dyn FnOnce() + Send> = Box::new(move || {
        latch.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(wake) = &wake {
          wake();
        }
        drop(hold);
      });
      let job =
        CellJob { owner: font, bytes: face.bytes().clone(), request, priority: JobPriority::Needed, done: Some(done) };
      let submitted = st.worker.as_ref().is_some_and(|w| w.submit(job));
      if submitted {
        entry.requested.extend(missing);
      }
      submitted
    }
  };
  if !submitted {
    let error = Exception::from_message(ctx.clone(), "requestGlyphs: the glyph worker is not running")?;
    reject.call::<_, ()>((error,))?;
    return Ok(promise);
  }
  st.pending.borrow_mut().push(PendingRequest {
    font,
    glyphs,
    resolve: Persistent::save(&ctx, resolve),
    reject: Persistent::save(&ctx, reject),
  });
  // Already-made glyphs settle now rather than a frame later.
  settle_pending(&ctx, &st);
  Ok(promise)
}

fn glyph_cells(ctx: Ctx<'_>, font: u64, glyphs: Vec<f64>) -> rquickjs::Result<Array<'_>> {
  let glyphs = glyph_ids(&ctx, "glyphCells", glyphs)?;
  let st = state(&ctx);
  let fonts = st.fonts.borrow();
  let entry = fonts.get(&font).ok_or_else(|| throw_str(&ctx, &format!("glyphCells: unknown font {font}")))?;
  cells_array(&ctx, entry, &glyphs)
}

// `{ glyph, x, y, width, height, left, top }` per requested glyph, null
// for one not in the atlas.
fn cells_array<'js>(ctx: &Ctx<'js>, entry: &FontEntry, glyphs: &[u16]) -> rquickjs::Result<Array<'js>> {
  let array = Array::new(ctx.clone())?;
  for (i, glyph) in glyphs.iter().enumerate() {
    match entry.atlas.packer().placement(*glyph) {
      Some(placement) => array.set(i, cell_object(ctx, *glyph, placement)?)?,
      None => array.set(i, rquickjs::Value::new_null(ctx.clone()))?,
    }
  }
  Ok(array)
}

fn cell_object<'js>(ctx: &Ctx<'js>, glyph: u16, placement: CellPlacement) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("glyph", glyph as u32)?;
  obj.set("x", placement.x)?;
  obj.set("y", placement.y)?;
  obj.set("width", placement.width)?;
  obj.set("height", placement.height)?;
  obj.set("left", placement.left)?;
  obj.set("top", placement.top)?;
  Ok(obj)
}

/// Reject a request's promise with an Error carrying `msg`.
fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, msg: &str) {
  let (Ok(func), Ok(error)) = (reject.restore(ctx), Exception::from_message(ctx.clone(), msg)) else {
    return;
  };
  if let Err(e) = func.call::<_, ()>((error,)) {
    log::warn!("[font] reject call failed: {e}");
  }
}

// Settle every pending request whose glyphs are all placed or failed. The
// cells array is built before the fonts borrow ends and the resolve runs:
// the callback may call back into this module.
fn settle_pending(ctx: &Ctx<'_>, st: &Rc<Inner>) {
  if st.pending.borrow().is_empty() {
    return;
  }
  let pending = std::mem::take(&mut *st.pending.borrow_mut());
  for request in pending {
    enum Outcome<'js> {
      Wait,
      Gone,
      Ready(Array<'js>),
    }
    let outcome = {
      let fonts = st.fonts.borrow();
      match fonts.get(&request.font) {
        None => Outcome::Gone,
        Some(entry) => {
          let done =
            request.glyphs.iter().all(|g| entry.atlas.packer().placement(*g).is_some() || entry.failed.contains(g));
          if done {
            match cells_array(ctx, entry, &request.glyphs) {
              Ok(array) => Outcome::Ready(array),
              Err(e) => {
                log::warn!("[font] building a cells result failed: {e}");
                Outcome::Wait
              }
            }
          } else {
            Outcome::Wait
          }
        }
      }
    };
    match outcome {
      Outcome::Wait => st.pending.borrow_mut().push(request),
      Outcome::Gone => reject_with(ctx, request.reject, "requestGlyphs: the font was destroyed"),
      Outcome::Ready(array) => {
        let settle = || -> rquickjs::Result<()> { request.resolve.restore(ctx)?.call::<_, ()>((array,)) };
        if let Err(e) = settle() {
          log::warn!("[font] resolve call failed: {e}");
        }
      }
    }
  }
}

/// Per-frame hook (see `frame::advance`): land the cells the worker made
/// in their atlases, upload, and settle the requests they complete.
/// Returns true when an atlas texture changed, so the caller requests a
/// redraw.
pub(crate) fn tick(ctx: &Ctx<'_>) -> bool {
  let Some(state) = ctx.userdata::<FontPluginState>() else {
    return false;
  };
  let st = &state.0;
  let done = st.worker.as_ref().map(|w| w.drain()).unwrap_or_default();
  let mut uploaded = false;
  if !done.is_empty() {
    let mut fonts = st.fonts.borrow_mut();
    let mut changed = HashSet::new();
    for batch in done {
      // A font destroyed while its cells were in the making: nothing to land.
      let Some(entry) = fonts.get_mut(&batch.owner) else {
        continue;
      };
      for cell in batch.cells {
        let glyph = cell.key;
        if entry.atlas.insert(cell) == InsertOutcome::Full {
          log::warn!(
            "[font] atlas of font {} is full at its {}x{} cap; glyph {glyph} stays missing",
            batch.owner,
            entry.atlas.packer().size().0,
            entry.atlas.packer().size().1
          );
          entry.failed.insert(glyph);
        }
      }
      entry.failed.extend(batch.failed);
      changed.insert(batch.owner);
    }
    for id in changed {
      let Some(entry) = fonts.get_mut(&id) else {
        continue;
      };
      match entry.atlas.flush(&st.gui.alloy) {
        Ok(sent) => uploaded |= sent,
        Err(e) => log::warn!("[font] atlas upload failed: {e}"),
      }
    }
  }
  settle_pending(ctx, st);
  uploaded
}
