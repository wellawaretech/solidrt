//! The text layer's draw vocabulary: the quads a `<text>` hands the glyph
//! pass, and the policy that turns a mask's coverage into color. Plain
//! data shared by the UI thread (which builds the quads from the text
//! atlas's placements) and the raster thread (which draws them).

/// One glyph of a text layer: where its cell lands in the layer, which
/// atlas texels it samples, and the run's color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphQuad {
  /// The quad in layer pixels: x, y from the layer's top-left corner (y
  /// down), width, height. Whole pixels: the subpixel part of the pen
  /// position is in the cell's phase.
  pub dst: [f32; 4],
  /// The cell in atlas texels: x, y, width, height.
  pub src: [f32; 4],
  /// The run's color with straight alpha in 0..1; the pass premultiplies.
  /// In a gradient group only the alpha counts.
  pub color: [f32; 4],
}

/// How a mask's coverage becomes color: the swash spike's four modes. A
/// layer is composited over whatever lies beneath it by the compositor's
/// plain sRGB source-over, so the modes are coverage remaps that assume a
/// background of the opposite polarity to the text (light text on a dark
/// ground, where the symptom lives), keyed on the text color's luminance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CoverageMode {
  /// Coverage as is: what Impeller does.
  #[default]
  Naive = 0,
  /// Linear-light blending against black for light text and white for
  /// dark, as a remap: `c ^ (1 / gamma)` and `1 - (1 - c) ^ (1 / gamma)`.
  LinearLight = 1,
  /// Light text mirrors the curve naive blending gives dark text, so both
  /// polarities carry equal weight: `(1 - (1 - c) ^ gamma) ^ (1 / gamma)`.
  PolarityRemap = 2,
  /// Linear light for light text, naive for dark (which keeps its
  /// implicit darkening).
  PolarityLinear = 3,
}

/// The exponent the remaps map through: the sRGB-ish gamma the spike
/// tuned around.
pub const DEFAULT_COVERAGE_GAMMA: f32 = 2.2;

/// The coverage-to-color policy of a text layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoveragePolicy {
  pub mode: CoverageMode,
  pub gamma: f32,
}

impl Default for CoveragePolicy {
  fn default() -> Self {
    Self { mode: CoverageMode::Naive, gamma: DEFAULT_COVERAGE_GAMMA }
  }
}

/// Texels across a gradient ramp: the stop colors sampled at this many
/// positions along 0..1, the steps 8-bit output can show.
pub const RAMP_WIDTH: usize = 256;

/// The shape of a text run's gradient; the discriminants are the pass's
/// `uGradient` values (0 is no gradient).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
  /// The parameter is the mapped x.
  Linear = 1,
  /// The parameter is the mapped point's distance from the origin.
  Radial = 2,
}

/// What a gradient does past its ends (Impeller's tile modes); the
/// discriminants are the pass's `uTile` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientTile {
  Clamp = 0,
  Repeat = 1,
  Mirror = 2,
  /// Transparent outside 0..1.
  Decal = 3,
}

/// A run's gradient paint resolved to the layer (rendertree/text/
/// gradient.rs): how a layer pixel maps to the gradient's parameter, and
/// the colors along it.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphGradient {
  pub kind: GradientKind,
  /// Layer pixel (x, y) to gradient space as two affine rows over
  /// (x, y, 1): x' from `x_row`, y' from `y_row`. A linear gradient reads
  /// its parameter as x', a radial one as the length of (x', y').
  pub x_row: [f32; 3],
  pub y_row: [f32; 3],
  pub tile: GradientTile,
  /// The stop colors sampled along the parameter: `RAMP_WIDTH` texels of
  /// straight-alpha sRGB rgba8, the ramp texture the pass samples.
  pub ramp: Vec<[u8; 4]>,
}

/// The quads of a text layer that share one paint: their own solid colors,
/// or `gradient` evaluated per fragment (a quad's alpha still applies).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphGroup {
  pub quads: Vec<GlyphQuad>,
  pub gradient: Option<GlyphGradient>,
}
