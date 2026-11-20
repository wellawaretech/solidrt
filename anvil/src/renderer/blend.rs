//! The blend modes as anvil draws them, split by what GL's fixed-function
//! blend stage expresses on premultiplied color: fifteen modes are a
//! `glBlendFunc` factor pair (`gl_factors`), set per batch; the other
//! fourteen are evaluated in the composite program against a copy of the
//! destination (`shader_mode`, exec.rs COMPOSITE_BLEND), the op drawn into
//! an implicit layer first (walk.rs). Pure functions, so the tests read
//! them without a context.

use crate::BlendMode;

// The shader modes, as the composite program's `uBlend` takes them:
// `BlendMode`'s discriminants. The `const int BLEND_*` declarations in
// exec.rs's COMPOSITE_BLEND must match them.
/// The composite program's identity mode: it returns the layer's color
/// unblended, which the fixed-function blend then composites (a layer with
/// a color matrix or a blur under source-over or another fixed-function
/// mode). SourceOver's discriminant; the GLSL const of the same name in
/// exec.rs's COMPOSITE_BLEND must match.
pub const BLEND_SOURCE_OVER: i32 = 3;
pub const BLEND_OVERLAY: i32 = 15;
pub const BLEND_DARKEN: i32 = 16;
pub const BLEND_LIGHTEN: i32 = 17;
pub const BLEND_COLOR_DODGE: i32 = 18;
pub const BLEND_COLOR_BURN: i32 = 19;
pub const BLEND_HARD_LIGHT: i32 = 20;
pub const BLEND_SOFT_LIGHT: i32 = 21;
pub const BLEND_DIFFERENCE: i32 = 22;
pub const BLEND_EXCLUSION: i32 = 23;
pub const BLEND_MULTIPLY: i32 = 24;
pub const BLEND_HUE: i32 = 25;
pub const BLEND_SATURATION: i32 = 26;
pub const BLEND_COLOR: i32 = 27;
pub const BLEND_LUMINOSITY: i32 = 28;

/// The (source, destination) factors `mode` blends by on premultiplied
/// color, the same for the color and the alpha channels; None for the
/// fourteen modes no factor pair expresses, which the composite program
/// evaluates (`shader_mode`). Multiply is among those: on premultiplied
/// color it is `src * (1 - da) + dst * (1 - sa) + src * dst`. Plus clamps
/// in the unorm8 target, as Skia's does. Destination draws nothing; the
/// walk skips its ops, so its pair is never set.
pub fn gl_factors(mode: BlendMode) -> Option<(u32, u32)> {
  use glow::{
    DST_ALPHA, ONE, ONE_MINUS_DST_ALPHA, ONE_MINUS_SRC_ALPHA, ONE_MINUS_SRC_COLOR, SRC_ALPHA, SRC_COLOR, ZERO,
  };
  let factors = match mode {
    BlendMode::Clear => (ZERO, ZERO),
    BlendMode::Source => (ONE, ZERO),
    BlendMode::Destination => (ZERO, ONE),
    BlendMode::SourceOver => (ONE, ONE_MINUS_SRC_ALPHA),
    BlendMode::DestinationOver => (ONE_MINUS_DST_ALPHA, ONE),
    BlendMode::SourceIn => (DST_ALPHA, ZERO),
    BlendMode::DestinationIn => (ZERO, SRC_ALPHA),
    BlendMode::SourceOut => (ONE_MINUS_DST_ALPHA, ZERO),
    BlendMode::DestinationOut => (ZERO, ONE_MINUS_SRC_ALPHA),
    BlendMode::SourceATop => (DST_ALPHA, ONE_MINUS_SRC_ALPHA),
    BlendMode::DestinationATop => (ONE_MINUS_DST_ALPHA, SRC_ALPHA),
    BlendMode::Xor => (ONE_MINUS_DST_ALPHA, ONE_MINUS_SRC_ALPHA),
    BlendMode::Plus => (ONE, ONE),
    BlendMode::Modulate => (ZERO, SRC_COLOR),
    BlendMode::Screen => (ONE, ONE_MINUS_SRC_COLOR),
    BlendMode::Overlay
    | BlendMode::Darken
    | BlendMode::Lighten
    | BlendMode::ColorDodge
    | BlendMode::ColorBurn
    | BlendMode::HardLight
    | BlendMode::SoftLight
    | BlendMode::Difference
    | BlendMode::Exclusion
    | BlendMode::Multiply
    | BlendMode::Hue
    | BlendMode::Saturation
    | BlendMode::Color
    | BlendMode::Luminosity => return None,
  };
  Some(factors)
}

/// The composite program's `uBlend` for a shader mode (a `BLEND_*`); None
/// for the fifteen fixed-function modes (`gl_factors`).
pub fn shader_mode(mode: BlendMode) -> Option<i32> {
  let value = match mode {
    BlendMode::Overlay => BLEND_OVERLAY,
    BlendMode::Darken => BLEND_DARKEN,
    BlendMode::Lighten => BLEND_LIGHTEN,
    BlendMode::ColorDodge => BLEND_COLOR_DODGE,
    BlendMode::ColorBurn => BLEND_COLOR_BURN,
    BlendMode::HardLight => BLEND_HARD_LIGHT,
    BlendMode::SoftLight => BLEND_SOFT_LIGHT,
    BlendMode::Difference => BLEND_DIFFERENCE,
    BlendMode::Exclusion => BLEND_EXCLUSION,
    BlendMode::Multiply => BLEND_MULTIPLY,
    BlendMode::Hue => BLEND_HUE,
    BlendMode::Saturation => BLEND_SATURATION,
    BlendMode::Color => BLEND_COLOR,
    BlendMode::Luminosity => BLEND_LUMINOSITY,
    _ => return None,
  };
  Some(value)
}

/// Whether a fully transparent source leaves the destination unchanged
/// under `mode`, so the walk may skip a draw whose color has alpha 0: a
/// fixed-function mode whose destination factor is one at a zero source
/// (ONE, ONE_MINUS_SRC_ALPHA, ONE_MINUS_SRC_COLOR), and every shader mode
/// (each formula returns the destination for a zero source). False for
/// Clear, Source, SourceIn, DestinationIn, SourceOut, DestinationATop and
/// Modulate, which clear the destination under a zero source.
pub fn keeps_destination(mode: BlendMode) -> bool {
  match gl_factors(mode) {
    Some((_, dst)) => matches!(dst, glow::ONE | glow::ONE_MINUS_SRC_ALPHA | glow::ONE_MINUS_SRC_COLOR),
    None => true,
  }
}
