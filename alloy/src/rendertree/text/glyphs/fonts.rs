// The registered fonts as the glyph engine sees them: the one reader of
// the font bytes. A face keeps the bytes (shared with the worker thread,
// which parses them with skrifa per job), a harfrust font at the default
// location, instanced on the weight axis of a variable font per requested
// weight so a `fontWeight: 700` run shapes with the 700 advances rather
// than the regular ones, and on its width axis per requested stretch, the
// underline metrics the decoration path draws with, and the autohinter's
// glyph styles (its per-glyph script classification, invariant per font)
// derived here once so a rasterizer on either thread hints without
// deriving them inside a frame. Resolution by name: the alias first, then
// the family names `family_names` reads, first registration winning.
use crate::impellers::FontWeight;
use crate::rendertree::text::UnderlineMetrics;
use crate::rendertree::FontPayload;
use harfrust::font::Variation as HarfVariation;
use harfrust::{Font, Tag};
use skrifa::instance::{Location, LocationRef, Size};
use skrifa::outline::GlyphStyles;
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider, Tag as SkrifaTag};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The weight axis tag of a variable font (OpenType `wght`).
const WEIGHT_AXIS: &[u8; 4] = b"wght";
/// The width axis tag (OpenType `wdth`): CSS font-stretch's percentage,
/// 100 normal.
const WIDTH_AXIS: &[u8; 4] = b"wdth";
/// The italic axis tag (OpenType `ital`): a font with one needs no
/// synthetic slant.
const ITALIC_AXIS: &[u8; 4] = b"ital";
/// The weights Impeller's enum spans, CSS numbers: Thin is 100, Black 900.
const WEIGHT_STEP: u16 = 100;
/// A weight at or above this is drawn emboldened when the font has no
/// weight axis (the synthetic bold every renderer applies to a static
/// regular face).
const SYNTHETIC_BOLD_FROM: u16 = 600;

/// Index of a face in the set: stable for the set's life.
pub type FaceId = usize;

/// The shared font bytes: what harfrust and skrifa parse. A borrowed
/// payload (the embedded Notos) costs no copy; an owned one (a packed
/// trailer font) is moved in once.
pub type FontBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// One registered font file.
pub struct Face {
  bytes: FontBytes,
  /// The font at its default location; instances derive from it.
  font: Font,
  units_per_em: f32,
  /// The weight axis range when the font is variable in weight.
  weight_axis: Option<(f32, f32)>,
  /// The width axis range when the font is variable in width.
  width_axis: Option<(f32, f32)>,
  /// Whether the font carries a real italic axis.
  italic_axis: bool,
  /// The font's underline position and thickness, in em.
  underline: UnderlineMetrics,
  /// The autohinter's per-glyph styles (an Arc inside: a clone is the
  /// same set), what every hinter of the face is built with.
  styles: GlyphStyles,
  /// Registered under a role alias ("sans", "serif", "mono"): what fallback
  /// tries before the faces registered by family name alone.
  role: bool,
  /// Instances per (weight, width bits), built on first use (deriving one
  /// shares the parsed tables; only the location differs). Interior
  /// mutability so a set borrowed shared (the platform's `glyphs()`) still
  /// instances.
  instances: RefCell<HashMap<(u16, u32), Font>>,
}

impl Face {
  /// The bytes, for the rasterizer's and the metrics' skrifa parse.
  pub fn bytes(&self) -> &FontBytes {
    &self.bytes
  }

  /// The autohinter's glyph styles of the face, for the rasterizer's
  /// hinters (handed to a worker job with the bytes).
  pub fn styles(&self) -> &GlyphStyles {
    &self.styles
  }

  pub fn units_per_em(&self) -> f32 {
    self.units_per_em
  }

  /// The weight axis value a requested weight resolves to, clamped into
  /// the axis range; None for a static font.
  pub fn weight_setting(&self, weight: u16) -> Option<f32> {
    self.weight_axis.map(|(min, max)| (weight as f32).clamp(min, max))
  }

  /// The width axis value a requested stretch (CSS font-stretch's
  /// percentage, 100 normal) resolves to, clamped into the axis range;
  /// None for a font without a width axis, which draws at its one width.
  pub fn width_setting(&self, stretch: f32) -> Option<f32> {
    self.width_axis.map(|(min, max)| stretch.clamp(min, max))
  }

  /// Whether a requested weight needs synthetic emboldening: the font has
  /// no weight axis and the weight is bold-class.
  pub fn synthetic_bold(&self, weight: u16) -> bool {
    self.weight_axis.is_none() && weight >= SYNTHETIC_BOLD_FROM
  }

  /// Whether an italic run needs a synthetic slant (no italic axis).
  pub fn synthetic_italic(&self) -> bool {
    !self.italic_axis
  }

  /// The glyph the face's character map gives `ch`, None when the face has
  /// none for it (what a warm-up filters a repertoire through, and what
  /// fallback asks of each face in turn).
  pub fn glyph_id(&self, ch: char) -> Option<u16> {
    let id = self.font.charmap().map_unicode(ch)?.to_u32() as u16;
    (id != 0).then_some(id)
  }

  /// The face's underline geometry, from its `post` table.
  pub fn underline(&self) -> UnderlineMetrics {
    self.underline
  }

  /// The harfrust font instanced at `weight` and `stretch` (the default
  /// location for a static font), built once per pair. A clone shares the
  /// instance.
  pub fn instance(&self, weight: u16, stretch: f32) -> Font {
    let axes: Vec<HarfVariation> = [
      self.weight_setting(weight).map(|value| HarfVariation { tag: Tag::new(WEIGHT_AXIS), value }),
      self.width_setting(stretch).map(|value| HarfVariation { tag: Tag::new(WIDTH_AXIS), value }),
    ]
    .into_iter()
    .flatten()
    .collect();
    if axes.is_empty() {
      return self.font.clone();
    }
    self
      .instances
      .borrow_mut()
      .entry((weight, stretch.to_bits()))
      .or_insert_with(|| self.font.instance_builder().variations(axes).build())
      .clone()
  }
}

/// Every registered face, resolvable by alias or family name.
#[derive(Default)]
pub struct FontSet {
  faces: Vec<Face>,
  by_name: HashMap<String, FaceId>,
}

/// The family alias every client registers its default face under.
const DEFAULT_ROLE: &str = "sans";

impl FontSet {
  /// Register `bytes` under `alias` and the family names the file
  /// declares. Errs, registering nothing, when the bytes are not a font
  /// either parser reads; the caller decides what that costs (a panic at
  /// startup, a warning mid-session).
  pub fn register(&mut self, bytes: FontBytes, alias: Option<&str>) -> Result<(), String> {
    let Some(font) = Font::new(bytes.clone(), 0) else {
      return Err("not a font file (no readable table directory)".to_string());
    };
    let Ok(skrifa_font) = FontRef::from_index(bytes.as_ref().as_ref(), 0) else {
      return Err("not a font file (unreadable by the rasterizer)".to_string());
    };
    let mut weight_axis = None;
    let mut width_axis = None;
    let mut italic_axis = false;
    for axis in skrifa_font.axes().iter() {
      if axis.tag() == SkrifaTag::new(WEIGHT_AXIS) {
        weight_axis = Some((axis.min_value(), axis.max_value()));
      } else if axis.tag() == SkrifaTag::new(WIDTH_AXIS) {
        width_axis = Some((axis.min_value(), axis.max_value()));
      } else if axis.tag() == SkrifaTag::new(ITALIC_AXIS) {
        italic_axis = true;
      }
    }
    let families = family_names(&skrifa_font);
    let underline = underline_metrics(&skrifa_font);
    let styles = GlyphStyles::new(&skrifa_font.outline_glyphs());
    let id = self.faces.len();
    self.faces.push(Face {
      bytes,
      units_per_em: font.units_per_em() as f32,
      font,
      weight_axis,
      width_axis,
      italic_axis,
      underline,
      styles,
      role: alias.is_some(),
      instances: RefCell::new(HashMap::new()),
    });
    for key in alias.map(str::to_string).into_iter().chain(families) {
      self.by_name.entry(key).or_insert(id);
    }
    Ok(())
  }

  /// Register every payload in order; `on_error` hears of each one that is
  /// not a font (alias, reason) and the set goes on without it.
  pub fn from_payloads(fonts: &[FontPayload], on_error: impl Fn(&str, &str)) -> Self {
    let mut set = Self::default();
    for FontPayload { alias, bytes } in fonts {
      let bytes: FontBytes = Arc::new(bytes.clone());
      if let Err(e) = set.register(bytes, alias.as_deref()) {
        on_error(alias.as_deref().unwrap_or("<unaliased>"), &e);
      }
    }
    set
  }

  /// The face a family name resolves to: the alias or family name as
  /// registered, else the fallback face - the one registered under the
  /// "sans" role (what every client registers its default face as), else
  /// the first face. None only for an empty set.
  pub fn resolve(&self, family: &str) -> Option<FaceId> {
    self
      .by_name
      .get(family)
      .or_else(|| self.by_name.get(DEFAULT_ROLE))
      .copied()
      .or_else(|| (!self.faces.is_empty()).then_some(0))
  }

  pub fn face(&self, id: FaceId) -> Option<&Face> {
    self.faces.get(id)
  }

  /// The face a cluster `primary` cannot shape is re-shaped on: the first
  /// face other than `primary` whose character map has `ch`, the faces
  /// registered under a role alias first, then the rest, each in
  /// registration order. None when no registered face covers `ch`.
  pub fn fallback_face(&self, primary: FaceId, ch: char) -> Option<FaceId> {
    let candidates = self.faces.iter().enumerate().filter(|(id, _)| *id != primary);
    let roles = candidates.clone().filter(|(_, face)| face.role);
    let others = candidates.filter(|(_, face)| !face.role);
    roles.chain(others).find(|(_, face)| face.glyph_id(ch).is_some()).map(|(id, _)| id)
  }

  pub fn is_empty(&self) -> bool {
    self.faces.is_empty()
  }
}

/// Impeller's weight enum as the CSS number the axis and the synthetic
/// bold rule read: Thin 100 through Black 900.
pub fn weight_value(weight: FontWeight) -> u16 {
  (weight as u16 + 1) * WEIGHT_STEP
}

// A face's underline geometry in em, from its `post` table; the shipped
// Notos' values for a face without one.
fn underline_metrics(font: &FontRef<'_>) -> UnderlineMetrics {
  let metrics = font.metrics(Size::unscaled(), LocationRef::default());
  let Some(underline) = metrics.underline else {
    return UnderlineMetrics::DEFAULT;
  };
  let upem = metrics.units_per_em as f32;
  UnderlineMetrics { position: -underline.offset / upem, thickness: underline.thickness / upem }
}

/// The names a font registers under besides its alias: every family
/// record of its `name` table, then every typographic family record, each
/// in table order, decoded where the encoding is known (Unicode and Mac
/// Roman; a record in another encoding decodes empty and is skipped).
pub fn family_names(font: &FontRef<'_>) -> Vec<String> {
  [StringId::FAMILY_NAME, StringId::TYPOGRAPHIC_FAMILY_NAME]
    .into_iter()
    .flat_map(|id| font.localized_strings(id))
    .map(|s| s.to_string())
    .filter(|name| !name.is_empty())
    .collect()
}

/// The location in the font's variation space for a weight and a width
/// axis setting (every other axis at its default), as the metrics and the
/// outlines read it.
pub(super) fn axis_location(font: &FontRef<'_>, weight: Option<f32>, width: Option<f32>) -> Location {
  let settings =
    [weight.map(|value| (SkrifaTag::new(WEIGHT_AXIS), value)), width.map(|value| (SkrifaTag::new(WIDTH_AXIS), value))];
  font.axes().location(settings.into_iter().flatten())
}
