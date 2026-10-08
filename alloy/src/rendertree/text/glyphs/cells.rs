// A glyph as pixels: the cell an atlas packs. Two kinds, chosen per atlas:
//
// - `Mask`: the glyph's coverage at an exact pixel size, rasterized by zeno
//   from its outline at a subpixel phase. Drawn 1:1 it is what a text
//   renderer and a terminal grid want; stored as premultiplied white
//   (coverage in every channel) so a consumer that samples it as a plain
//   texture draws white text to tint.
// - `Msdf`: a multi-channel signed distance field at a fixed size per em
//   with a distance range in texels, generated from the outline by msdfgen
//   (msdf.rs, over the shim build.rs compiles with the vendored core). One
//   cell serves every zoom; a consumer decodes the field in its shader.
//
// The outline comes from skrifa (fontations' scaler, what the shaper reads
// the font with too), hinted by its port of FreeType's autohinter when the
// request asks: the font's own instructions are never run (the shipped
// Notos have none), the autohinter is forced on in its light mode, which
// snaps x-height and cap height to pixel rows and leaves advances and x
// extents alone, so layout and the subpixel phases are untouched.
//
// Rasterization is CPU work (a few hundred microseconds per glyph) and runs
// on the worker thread, or on the UI thread within the text atlas's budget;
// a `Rasterizer` owns its thread's hinter cache and raster scratch.
use super::fonts::{axis_location, FontBytes};
use super::msdf::msdf_cell;
use super::outline::Outline;
use skrifa::instance::{Location, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, SmoothMode, Target};
use skrifa::outline::{GlyphStyles, OutlineGlyphCollection};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use zeno::{Fill, Format, Mask, Origin, Placement, Scratch, Vector};

/// Bytes per atlas texel: cells are rgba8 whatever their kind.
pub const BYTES_PER_TEXEL: usize = 4;
/// The slant of a synthetic italic, degrees from vertical: the angle every
/// renderer uses for a face without an italic (FreeType, Skia, Impeller).
const SYNTHETIC_ITALIC_DEGREES: f32 = 12.0;
/// Synthetic bold outset as a fraction of the pixel size, per side: the
/// stem darkening the swash spike settled on for a bold-class weight on a
/// static regular face.
const SYNTHETIC_BOLD_STRENGTH: f32 = 0.04;
/// The autohinter's target per `Hint` mode. The light target moves points
/// vertically only (x-height and cap height onto pixel rows, stems left
/// alone so the subpixel phases keep working), the advances kept linear.
/// The full target is the mono one: stem widths and positions rounded to
/// whole pixels (FreeType's grayscale "normal" target only nearly aligns
/// them, a 1.1 px stem filling one column and spilling the rest), the
/// outline still rasterized with anti-aliasing, so stems are solid and
/// curves stay smooth. Layout reads its advances from the shaper either
/// way, so it never depends on the display scale; a full-hinted glyph is
/// drawn at the unhinted pen.
const LIGHT_TARGET: Target =
  Target::Smooth { mode: SmoothMode::Light, symmetric_rendering: true, preserve_linear_metrics: true };
const FULL_TARGET: Target = Target::Mono;

/// How a mask cell's outline is hinted: the autohinter (FreeType's, as
/// skrifa ports it), never the font's own instructions, which the shipped
/// fonts do not carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Hint {
  /// The outline as the font draws it: what a display at 2x and up gets,
  /// and the faithful shape for text that scales.
  #[default]
  Off,
  /// Baseline, x-height and cap height onto pixel rows, x untouched so
  /// the subpixel phases keep working: Chrome's look on Linux.
  Light,
  /// Stem widths and positions onto whole pixels as well, at the cost of
  /// letterforms rounded per size; a cell is made at one phase and placed
  /// at a whole pixel. A terminal grid, a 1x screen read from a distance.
  Full,
}

/// What kind of pixels an atlas holds, fixed at its creation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellKind {
  /// Coverage masks at `ppem` pixels per em.
  Mask { ppem: f32 },
  /// Distance fields at `ppem` texels per em, `range` texels wide.
  Msdf { ppem: f32, range: f32 },
}

impl CellKind {
  /// Pixels (texels) per em: the scale every cell of the kind is made at.
  pub fn ppem(&self) -> f32 {
    match self {
      CellKind::Mask { ppem } | CellKind::Msdf { ppem, .. } => *ppem,
    }
  }
}

/// A rasterized glyph: its pixels and where its box sits relative to the
/// glyph origin, in texels (`left` right of the origin, `top` above the
/// baseline). Keyed by whatever its owner keys cells on; the rasterizer
/// keys on the glyph id.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell<K = u16> {
  pub key: K,
  pub width: u32,
  pub height: u32,
  pub left: i32,
  pub top: i32,
  /// `width * height * BYTES_PER_TEXEL` bytes, rgba8, rows top to bottom.
  pub pixels: Vec<u8>,
}

impl<K> Cell<K> {
  /// The same pixels under another key: how an owner files a glyph-keyed
  /// cell under its own identity for it.
  pub fn with_key<K2>(self, key: K2) -> Cell<K2> {
    Cell { key, width: self.width, height: self.height, left: self.left, top: self.top, pixels: self.pixels }
  }
}

/// What a job rasterizes: one face at one location and style, for a cell
/// kind, over a list of glyphs.
#[derive(Clone, Debug)]
pub struct CellRequest {
  pub kind: CellKind,
  /// The weight axis value, None for a face without the axis.
  pub weight: Option<f32>,
  /// The width axis value, None for a face without the axis.
  pub width: Option<f32>,
  /// Synthetic bold outset (no weight axis, bold-class weight).
  pub synthetic_bold: bool,
  /// Synthetic slant (an italic run on a face without an italic axis).
  pub synthetic_italic: bool,
  /// The subpixel x offset the outline is rasterized at, in pixels within
  /// 0..1: a mask drawn at a pen position between pixels keeps its shape
  /// when it is made at that fraction. Ignored by a distance field, which
  /// is placed by its sampler.
  pub phase: f32,
  /// Extra outset per side in pixels, the low-DPI stem darkening policy
  /// (zero for none); adds to a synthetic bold's.
  pub darken: f32,
  /// How the outline is hinted (the low-DPI policy). A mask only; a
  /// distance field is unhinted whatever this says.
  pub hint: Hint,
  pub glyphs: Vec<u16>,
}

/// The per-thread state cells are made with: the hinters built so far,
/// per font, size and location (building one derives the font's glyph
/// styles and blue zones, far more than a glyph costs), and zeno's
/// scratch memory.
#[derive(Default)]
pub struct Rasterizer {
  /// Keyed by the font's allocation, which the entry pins: a reset font
  /// set cannot hand the address to other bytes while a hinter for it is
  /// cached.
  fonts: HashMap<usize, FontHinters>,
  scratch: Scratch,
}

struct FontHinters {
  _bytes: FontBytes,
  /// The autohinter's per-glyph classification, invariant per font, so
  /// derived once and shared by every hinter of the font.
  styles: Option<GlyphStyles>,
  /// Per (ppem bits, normalized coordinates' bits, mode).
  by_instance: HashMap<(u32, Vec<i16>, Hint), HintingInstance>,
}

impl FontHinters {
  /// The hinter for `ppem` at `location`, built on first use; None when
  /// skrifa cannot hint the font, which then draws unhinted.
  fn hinter(
    &mut self,
    outlines: &OutlineGlyphCollection,
    ppem: f32,
    location: &Location,
    hint: Hint,
  ) -> Option<&HintingInstance> {
    let key = (ppem.to_bits(), location.coords().iter().map(|c| c.to_bits()).collect(), hint);
    match self.by_instance.entry(key) {
      Entry::Occupied(entry) => Some(entry.into_mut()),
      Entry::Vacant(entry) => {
        let styles = self.styles.get_or_insert_with(|| GlyphStyles::new(outlines)).clone();
        let target = if hint == Hint::Full { FULL_TARGET } else { LIGHT_TARGET };
        let options = HintingOptions { engine: Engine::Auto(Some(styles)), target };
        let hinter = HintingInstance::new(outlines, Size::new(ppem), location, options).ok()?;
        Some(entry.insert(hinter))
      }
    }
  }
}

impl Rasterizer {
  /// Rasterize every glyph of `request` from `bytes`. A glyph skrifa cannot
  /// draw (no outline, an id past the font) is left out; the caller
  /// reports those by their absence. None when the bytes are not a font.
  pub fn rasterize(&mut self, bytes: &FontBytes, request: &CellRequest) -> Option<Vec<Cell>> {
    let font = FontRef::from_index(bytes.as_ref().as_ref(), 0).ok()?;
    let outlines = font.outline_glyphs();
    let location = axis_location(&font, request.weight, request.width);
    let ppem = request.kind.ppem();
    let hinter = if request.hint != Hint::Off && matches!(request.kind, CellKind::Mask { .. }) {
      let key = Arc::as_ptr(bytes) as *const () as usize;
      let hinters = self.fonts.entry(key).or_insert_with(|| FontHinters {
        _bytes: bytes.clone(),
        styles: None,
        by_instance: HashMap::new(),
      });
      hinters.hinter(&outlines, ppem, &location, request.hint)
    } else {
      None
    };
    let outset = if request.synthetic_bold { ppem * SYNTHETIC_BOLD_STRENGTH } else { 0.0 } + request.darken;
    let mut cells = Vec::with_capacity(request.glyphs.len());
    for &glyph in &request.glyphs {
      let Some(outline_glyph) = outlines.get(GlyphId::new(glyph as u32)) else {
        continue;
      };
      let mut outline = Outline::default();
      let drawn = match hinter {
        Some(hinter) => outline_glyph.draw(DrawSettings::hinted(hinter, false), &mut outline),
        None => outline_glyph.draw(DrawSettings::unhinted(Size::new(ppem), &location), &mut outline),
      };
      if drawn.is_err() {
        continue;
      }
      if outset > 0.0 {
        outline.embolden(outset);
      }
      if request.synthetic_italic {
        outline.skew(SYNTHETIC_ITALIC_DEGREES);
      }
      let cell = match request.kind {
        CellKind::Mask { .. } => Some(mask_cell(glyph, &outline, request.phase, &mut self.scratch)),
        CellKind::Msdf { range, .. } => msdf_cell(glyph, outline.commands(), range),
      };
      if let Some(cell) = cell {
        cells.push(cell);
      }
    }
    Some(cells)
  }
}

/// The coverage mask of `outline` at the subpixel `phase`, as a
/// premultiplied-white rgba8 cell; a blank outline is an empty cell. The
/// box is the outline's bounds at the phase rounded out to pixels, placed
/// y up from the baseline (zeno's bottom-left origin).
fn mask_cell(glyph: u16, outline: &Outline, phase: f32, scratch: &mut Scratch) -> Cell {
  let mut coverage = Vec::new();
  let mut placement = Placement::default();
  if !outline.is_empty() {
    let offset = Vector::new(phase, 0.0);
    // `inspect` sizes the mask before the render, which reports the box
    // relative to the baseline only once it knows the height.
    placement = Mask::with_scratch(outline.commands(), scratch)
      .format(Format::Alpha)
      .origin(Origin::BottomLeft)
      .style(Fill::NonZero)
      .offset(offset)
      .render_offset(offset)
      .inspect(|format, width, height| coverage.resize(format.buffer_size(width, height), 0))
      .render_into(&mut coverage, None);
  }
  let texels = placement.width as usize * placement.height as usize;
  let mut pixels = Vec::with_capacity(texels * BYTES_PER_TEXEL);
  for &alpha in coverage.iter().take(texels) {
    pixels.extend_from_slice(&[alpha; BYTES_PER_TEXEL]);
  }
  Cell {
    key: glyph,
    width: placement.width,
    height: placement.height,
    left: placement.left,
    top: placement.top,
    pixels,
  }
}
