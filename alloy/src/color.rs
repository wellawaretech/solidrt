use crate::impellers::Color;

// Color facts for the platform: the CSS string grammar (csscolorparser) and
// the perceptual math (sRGB <-> oklab, mixing, brightness). The transition
// system interpolates through these conversions; the flux parseColor /
// mixColors / brightness bindings are thin wrappers over this module. One
// owner for color understanding - JS forwards strings untouched
// (okf/backlog/css-colors-in-rust.md).

/// Parse a CSS color string (hex, `rgb()`/`rgba()`, `hsl()`/`hsla()`,
/// `hwb()`, named colors) into an sRGB Color. The error names the offending
/// string; callers surface it at the write site (throw-in-dev policy).
pub fn parse_css(s: &str) -> Result<Color, String> {
  let c = csscolorparser::parse(s).map_err(|_| format!("Invalid color \"{s}\""))?;
  let [r, g, b, a] = c.to_array();
  Ok(Color::new_srgba(r, g, b, a))
}

/// Mix two colors in oklab; `t` is the fraction of `b` (0 = pure `a`,
/// 1 = pure `b`). Alpha mixes linearly alongside.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
  let la = color_to_oklab(a);
  let lb = color_to_oklab(b);
  let mut out = [0.0f32; 4];
  for i in 0..4 {
    out[i] = la[i] + (lb[i] - la[i]) * t;
  }
  oklab_to_color(out)
}

/// Perceived brightness, 0 (black) to 1 (white): the YIQ luma weighting
/// (matches the retired colord brightness(), so polarity decisions like
/// light-on-dark text keep their answers).
pub fn brightness(c: Color) -> f32 {
  0.299 * c.red + 0.587 * c.green + 0.114 * c.blue
}

// sRGB <-> oklab (Bjorn Ottosson's reference constants). Interpolating in
// oklab keeps perceptual lightness moving evenly, so color transitions and
// mixes avoid the desaturated middle a straight sRGB lerp produces. Alpha
// rides as its own linear lane ([L, a, b, alpha]).

fn srgb_to_linear(c: f32) -> f32 {
  if c <= 0.04045 {
    c / 12.92
  } else {
    ((c + 0.055) / 1.055).powf(2.4)
  }
}

fn linear_to_srgb(c: f32) -> f32 {
  if c <= 0.0031308 {
    c * 12.92
  } else {
    1.055 * c.powf(1.0 / 2.4) - 0.055
  }
}

pub fn color_to_oklab(c: Color) -> [f32; 4] {
  let r = srgb_to_linear(c.red);
  let g = srgb_to_linear(c.green);
  let b = srgb_to_linear(c.blue);
  let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
  let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
  let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
  [
    0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    c.alpha,
  ]
}

pub fn oklab_to_color(lanes: [f32; 4]) -> Color {
  let [lightness, a, b, alpha] = lanes;
  let l = lightness + 0.3963377774 * a + 0.2158037573 * b;
  let m = lightness - 0.1055613458 * a - 0.0638541728 * b;
  let s = lightness - 0.0894841775 * a - 1.2914855480 * b;
  let (l, m, s) = (l * l * l, m * m * m, s * s * s);
  let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
  let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
  let bl = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
  // Mid-mix values can leave the sRGB gamut (and springs can overshoot);
  // clamp per channel at the edge.
  Color::new_srgba(
    linear_to_srgb(r).clamp(0.0, 1.0),
    linear_to_srgb(g).clamp(0.0, 1.0),
    linear_to_srgb(bl).clamp(0.0, 1.0),
    alpha.clamp(0.0, 1.0),
  )
}
