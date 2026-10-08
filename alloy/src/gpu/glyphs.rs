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

/// How a mask's coverage becomes color: the swash spike's four modes and
/// DirectWrite's recipe. A layer is composited over whatever lies beneath
/// it by the compositor's plain sRGB source-over, so the modes are coverage
/// remaps that assume a background of the opposite polarity to the text
/// (light text on a dark ground, where the symptom lives), keyed on the
/// text color's luminance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CoverageMode {
  /// Coverage as is: what Impeller did.
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
  /// DirectWrite's grayscale blend as Windows Terminal publishes it
  /// (`dwrite_helpers.hlsl`, MIT): an enhanced-contrast boost of
  /// `contrast`, faded out as the text color brightens past 0.5 and gone
  /// at 0.75 lightness, then a gamma alpha correction from the text's
  /// intensity and the `gamma_ratios` of `gamma` (1.0 to 2.2, Windows's
  /// default 1.8). The default of `CoveragePolicy`.
  DirectWrite = 4,
}

/// The default policy's gamma: DirectWrite's Windows default, picked by eye
/// at 1x and 1.5x on 2026-10-08 (okf/plans/text-own-rasterizer.md, step
/// 4). The remap modes were tuned around 2.2 and want it set alongside.
pub const DEFAULT_COVERAGE_GAMMA: f32 = 1.8;
/// DirectWrite's default grayscale enhanced contrast.
pub const DEFAULT_COVERAGE_CONTRAST: f32 = 1.0;

/// The coverage-to-color policy of a text layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoveragePolicy {
  pub mode: CoverageMode,
  pub gamma: f32,
  /// The enhanced contrast of `CoverageMode::DirectWrite`; the other modes
  /// ignore it.
  pub contrast: f32,
}

impl Default for CoveragePolicy {
  fn default() -> Self {
    Self { mode: CoverageMode::DirectWrite, gamma: DEFAULT_COVERAGE_GAMMA, contrast: DEFAULT_COVERAGE_CONTRAST }
  }
}

// DirectWrite's alpha-correction constants per gamma, 1.0 to 2.2 in steps
// of a tenth (Windows Terminal's `DWrite_GetGammaRatios`, MIT), stored as
// the table has them, each over 4, scaled on read by the two 8-bit
// normalizers below.
const GAMMA_RATIO_TABLE: [[f32; 4]; 13] = [
  [0.0000, 0.0000, 0.0000, 0.0000],
  [0.0166, -0.0807, 0.2227, -0.0751],
  [0.0350, -0.1760, 0.4325, -0.1370],
  [0.0543, -0.2821, 0.6302, -0.1876],
  [0.0739, -0.3963, 0.8167, -0.2287],
  [0.0933, -0.5161, 0.9926, -0.2616],
  [0.1121, -0.6395, 1.1588, -0.2877],
  [0.1300, -0.7649, 1.3159, -0.3080],
  [0.1469, -0.8911, 1.4644, -0.3234],
  [0.1627, -1.0170, 1.6051, -0.3347],
  [0.1773, -1.1420, 1.7385, -0.3426],
  [0.1908, -1.2652, 1.8650, -0.3476],
  [0.2031, -1.3864, 1.9851, -0.3501],
];
const GAMMA_RATIO_TABLE_FIRST: f32 = 1.0;
const GAMMA_RATIO_TABLE_STEP: f32 = 0.1;
// The table's entries are quarters; the first and third ratios normalize a
// 16-bit product of two 8-bit values, the second and fourth an 8-bit one.
const GAMMA_RATIO_NORM_SQUARE: f64 = 65536.0 / (255.0 * 255.0) * 4.0;
const GAMMA_RATIO_NORM_LINEAR: f64 = 256.0 / 255.0 * 4.0;

impl CoveragePolicy {
  /// The alpha-correction constants the DirectWrite mode applies for this
  /// policy's gamma, clamped to the table's range.
  pub fn gamma_ratios(&self) -> [f32; 4] {
    let index = ((self.gamma - GAMMA_RATIO_TABLE_FIRST) / GAMMA_RATIO_TABLE_STEP).round();
    let index = (index.max(0.0) as usize).min(GAMMA_RATIO_TABLE.len() - 1);
    let r = GAMMA_RATIO_TABLE[index];
    [
      (GAMMA_RATIO_NORM_SQUARE * r[0] as f64 / 4.0) as f32,
      (GAMMA_RATIO_NORM_LINEAR * r[1] as f64 / 4.0) as f32,
      (GAMMA_RATIO_NORM_SQUARE * r[2] as f64 / 4.0) as f32,
      (GAMMA_RATIO_NORM_LINEAR * r[3] as f64 / 4.0) as f32,
    ]
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
