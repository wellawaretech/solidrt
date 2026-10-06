// The registered fonts as the glyph engine sees them. `build_typography`
// hands every `FontPayload` to Impeller's typography context and to the
// underline metrics table; this is the third reader of the same bytes. A
// face keeps the bytes (shared with the worker thread, which parses them
// with swash per job) and a harfrust font at the default location, and
// instances the weight axis of a variable font per requested weight, so a
// `fontWeight: 700` run shapes with the 700 advances rather than the
// regular ones. Resolution by name mirrors `FontMetricsTable::register`:
// the alias first, then the family and typographic family names, first
// registration winning, so the engine and the underline table never
// disagree on which file a family means.
use crate::impellers::FontWeight;
use crate::rendertree::FontPayload;
use harfrust::font::Variation as HarfVariation;
use harfrust::{Font, Tag};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The weight axis tag of a variable font (OpenType `wght`).
const WEIGHT_AXIS: &[u8; 4] = b"wght";
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

/// The shared font bytes: what read-fonts and swash parse. A borrowed
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
  /// Whether the font carries a real italic axis.
  italic_axis: bool,
  /// Weight instances, built on first use per weight (deriving one shares
  /// the parsed tables; only the location differs). Interior mutability so
  /// a set borrowed shared (the platform's `glyphs()`) still instances.
  instances: RefCell<HashMap<u16, Font>>,
}

impl Face {
  /// The bytes, for the worker's swash parse.
  pub fn bytes(&self) -> &FontBytes {
    &self.bytes
  }

  pub fn units_per_em(&self) -> f32 {
    self.units_per_em
  }

  /// The weight axis value a requested weight resolves to, clamped into
  /// the axis range; None for a static font.
  pub fn weight_setting(&self, weight: u16) -> Option<f32> {
    self.weight_axis.map(|(min, max)| (weight as f32).clamp(min, max))
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

  /// The harfrust font instanced at `weight` (the default location for a
  /// static font), built once per weight. A clone shares the instance.
  pub fn instance(&self, weight: u16) -> Font {
    let Some(value) = self.weight_setting(weight) else {
      return self.font.clone();
    };
    self
      .instances
      .borrow_mut()
      .entry(weight)
      .or_insert_with(|| {
        self.font.instance_builder().variations([HarfVariation { tag: Tag::new(WEIGHT_AXIS), value }]).build()
      })
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
  /// declares. A file that is not a font is skipped: the typography context
  /// reports that failure already, and a face the engine lacks just resolves
  /// to the fallback.
  pub fn register(&mut self, bytes: FontBytes, alias: Option<&str>) {
    let Some(font) = Font::new(bytes.clone(), 0) else {
      return;
    };
    let Ok(face) = ttf_parser::Face::parse(bytes.as_ref().as_ref(), 0) else {
      return;
    };
    let Some(swash_font) = swash::FontRef::from_index(bytes.as_ref().as_ref(), 0) else {
      return;
    };
    let mut weight_axis = None;
    let mut italic_axis = false;
    for axis in swash_font.variations() {
      if axis.tag() == swash::tag_from_bytes(WEIGHT_AXIS) {
        weight_axis = Some((axis.min_value(), axis.max_value()));
      } else if axis.tag() == swash::tag_from_bytes(ITALIC_AXIS) {
        italic_axis = true;
      }
    }
    let names = face.names();
    let families: Vec<String> = names
      .into_iter()
      .filter(|n| n.name_id == ttf_parser::name_id::FAMILY || n.name_id == ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
      .filter_map(|n| n.to_string())
      .collect();
    let id = self.faces.len();
    self.faces.push(Face {
      bytes,
      units_per_em: font.units_per_em() as f32,
      font,
      weight_axis,
      italic_axis,
      instances: RefCell::new(HashMap::new()),
    });
    for key in alias.map(str::to_string).into_iter().chain(families) {
      self.by_name.entry(key).or_insert(id);
    }
  }

  /// Register every payload in order (the typography context's order).
  pub fn from_payloads(fonts: &[FontPayload]) -> Self {
    let mut set = Self::default();
    for FontPayload { alias, bytes } in fonts {
      let bytes: FontBytes = Arc::new(bytes.clone());
      set.register(bytes, alias.as_deref());
    }
    set
  }

  /// The face a family name resolves to: the alias or family name as
  /// registered, else the fallback face - the one registered under the
  /// "sans" role (Impeller's own fallback is its default family, which the
  /// client registers as sans), else the first face. None only for an
  /// empty set.
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

  pub fn is_empty(&self) -> bool {
    self.faces.is_empty()
  }
}

/// Impeller's weight enum as the CSS number the axis and the synthetic
/// bold rule read: Thin 100 through Black 900.
pub fn weight_value(weight: FontWeight) -> u16 {
  (weight as u16 + 1) * WEIGHT_STEP
}
