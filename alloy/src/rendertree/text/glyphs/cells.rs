// A glyph as pixels: the cell an atlas packs. Two kinds, chosen per atlas:
//
// - `Mask`: the glyph's coverage at an exact pixel size, rasterized from
//   its outline by swash at a subpixel phase. Drawn 1:1 it is what a text
//   renderer and a terminal grid want; stored as premultiplied white
//   (coverage in every channel) so a consumer that samples it as a plain
//   texture draws white text to tint.
// - `Msdf`: a multi-channel signed distance field at a fixed size per em
//   with a distance range in texels, generated from the outline by msdfgen
//   (msdf.rs, over the shim build.rs compiles with the vendored core). One
//   cell serves every zoom; a consumer decodes the field in its shader.
//
// Rasterization is CPU work (a few hundred microseconds per glyph) and runs
// on the worker thread, or on the UI thread within the text atlas's budget;
// a `Rasterizer` owns the swash context its thread keeps, since swash
// scales through per-thread caches by design.
use super::msdf::msdf_cell;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Transform, Vector};
use swash::{FontRef, GlyphId};

/// Bytes per atlas texel: cells are rgba8 whatever their kind.
pub const BYTES_PER_TEXEL: usize = 4;
/// The slant of a synthetic italic, degrees from vertical: the angle every
/// renderer uses for a face without an italic (FreeType, Skia, Impeller).
const SYNTHETIC_ITALIC_DEGREES: f32 = 12.0;
/// Synthetic bold outset as a fraction of the pixel size, per side: the
/// stem darkening the swash spike settled on for a bold-class weight on a
/// static regular face.
const SYNTHETIC_BOLD_STRENGTH: f32 = 0.04;

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
/// baseline, both as swash places them). Keyed by whatever its owner keys
/// cells on; the rasterizer keys on the glyph id.
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
  /// Apply the font's hinting instructions to the outline (the low-DPI
  /// policy: x-height and cap height snap to pixel rows). A mask only; a
  /// distance field is unhinted whatever this says.
  pub hint: bool,
  pub glyphs: Vec<u16>,
}

/// The per-thread swash state cells are made with.
pub struct Rasterizer {
  context: ScaleContext,
}

impl Default for Rasterizer {
  fn default() -> Self {
    Self { context: ScaleContext::new() }
  }
}

impl Rasterizer {
  /// Rasterize every glyph of `request` from `bytes`. A glyph swash cannot
  /// render (no outline, an id past the font) is left out; the caller
  /// reports those by their absence. None when the bytes are not a font.
  pub fn rasterize(&mut self, bytes: &[u8], request: &CellRequest) -> Option<Vec<Cell>> {
    let font = FontRef::from_index(bytes, 0)?;
    let mut scaler = self
      .context
      .builder(font)
      .size(request.kind.ppem())
      .hint(request.hint && matches!(request.kind, CellKind::Mask { .. }))
      .variations(
        [
          request.weight.map(|value| swash::Setting { tag: swash::tag_from_bytes(b"wght"), value }),
          request.width.map(|value| swash::Setting { tag: swash::tag_from_bytes(b"wdth"), value }),
        ]
        .into_iter()
        .flatten(),
      )
      .build();
    let outset = |ppem: f32| if request.synthetic_bold { ppem * SYNTHETIC_BOLD_STRENGTH } else { 0.0 } + request.darken;
    let mut cells = Vec::with_capacity(request.glyphs.len());
    for &glyph in &request.glyphs {
      let cell = match request.kind {
        CellKind::Mask { ppem } => {
          let mut render = Render::new(&[Source::Outline]);
          render.format(Format::Alpha).offset(Vector::new(request.phase, 0.0));
          if request.synthetic_italic {
            render.transform(Some(Transform::skew(Angle::from_degrees(SYNTHETIC_ITALIC_DEGREES), Angle::ZERO)));
          }
          let strength = outset(ppem);
          if strength > 0.0 {
            render.embolden(strength);
          }
          render.render(&mut scaler, glyph as GlyphId).map(|image| mask_cell(glyph, &image))
        }
        CellKind::Msdf { ppem, range } => scaler.scale_outline(glyph as GlyphId).and_then(|mut outline| {
          let strength = outset(ppem);
          if strength > 0.0 {
            outline.embolden(strength, strength);
          }
          if request.synthetic_italic {
            outline.transform(&Transform::skew(Angle::from_degrees(SYNTHETIC_ITALIC_DEGREES), Angle::ZERO));
          }
          msdf_cell(glyph, &outline, range)
        }),
      };
      if let Some(cell) = cell {
        cells.push(cell);
      }
    }
    Some(cells)
  }
}

/// A coverage mask as a premultiplied-white rgba8 cell.
fn mask_cell(glyph: u16, image: &swash::scale::image::Image) -> Cell {
  let placement = image.placement;
  let texels = placement.width as usize * placement.height as usize;
  let mut pixels = Vec::with_capacity(texels * BYTES_PER_TEXEL);
  for &coverage in image.data.iter().take(texels) {
    pixels.extend_from_slice(&[coverage; BYTES_PER_TEXEL]);
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
