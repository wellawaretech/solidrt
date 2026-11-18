//! What a list executes: anvil's own ops, resolved at record time (colors
//! premultiplied, a paint reduced to what stage 1 draws), shared behind an
//! Arc so a retained list is recorded once and replayed by reference.

use super::record::DisplayList;
use super::{PaintShader, ShaderInput, Texture};
use crate::{Color, Matrix, Point, Rect, ShaderParam, StrokeCap, TextureSampling};
use crate::{CoveragePolicy, GlyphGradient, GlyphQuad};
use euclid::default::Transform2D;
use std::sync::Arc;

/// A premultiplied sRGB color: (r*a, g*a, b*a, a). Blending is
/// ONE, ONE_MINUS_SRC_ALPHA in sRGB with no linear step, the convention
/// the plan pins (what Impeller did on an RGBA8 target).
pub(super) type Premultiplied = [f32; 4];

/// `c` with its color channels scaled by its alpha, the form every op
/// carries and the blend expects.
pub(super) fn premultiply(c: Color) -> Premultiplied {
  [c.red * c.alpha, c.green * c.alpha, c.blue * c.alpha, c.alpha]
}

/// Four corner radii in CSS order: top-left, top-right, bottom-right,
/// bottom-left. Clamped at record time (`record::clamp_radii`) so adjacent
/// corners never overlap; all zero is a plain rect.
pub(super) type Radii = [f32; 4];

/// An image a shape is filled with: the texel rect of `texture` mapped onto
/// the draw's space by scale and translate (`to_texel` is that map's
/// inverse, local to texel, resolved at record time), sampled `sampling`,
/// clamped at the edges.
#[derive(Clone)]
pub(super) struct Image {
  pub texture: Texture,
  pub sampling: TextureSampling,
  pub to_texel: Transform2D<f32>,
}

/// One executable op of a list, in recording order.
pub(super) enum Op {
  Save,
  Restore,
  Translate {
    x: f32,
    y: f32,
  },
  Scale {
    x: f32,
    y: f32,
  },
  Transform(Matrix),
  /// Intersect only: the difference op is stage 5 and is dropped at
  /// record time with a warning.
  ClipRect(Rect),
  /// Intersect only, as `ClipRect`; `radii` in the rect's own space,
  /// clamped. The walk applies it in device space, one deep.
  ClipRoundedRect {
    rect: Rect,
    radii: Radii,
  },
  /// `stroke` None fills; Some(width) strokes centered on the edge, with
  /// miter joins (sharp corners) where a radius is zero. `radii` all zero
  /// is a plain rect. `image` Some fills the coverage with the image's
  /// texels, faded by `color`'s alpha.
  DrawRect {
    rect: Rect,
    radii: Radii,
    color: Premultiplied,
    stroke: Option<f32>,
    image: Option<Image>,
  },
  /// `stroke` None fills; Some(width) strokes centered on the ellipse.
  /// `image` as for `DrawRect`.
  DrawOval {
    bounds: Rect,
    color: Premultiplied,
    stroke: Option<f32>,
    image: Option<Image>,
  },
  /// A segment stroked `width` wide, centered on it, ended by `cap` at
  /// both ends. A width of zero or below draws nothing (the walk skips it).
  DrawLine {
    from: Point,
    to: Point,
    color: Premultiplied,
    width: f32,
    cap: StrokeCap,
  },
  /// `alpha` is the paint's alpha (1 without a paint): a texture's opacity
  /// is per op, not a group.
  DrawTexture {
    texture: Texture,
    src: Rect,
    dst: Rect,
    sampling: TextureSampling,
    alpha: f32,
  },
  /// A glyph run: `quads` over `atlas`, whose red channel is coverage,
  /// under `policy`, colored per quad or along `gradient`. `bounds` is the
  /// quads' union in the op's space, so the walk rejects a whole run that
  /// falls outside the clip before it looks at a glyph.
  DrawGlyphs {
    atlas: Texture,
    quads: Arc<[GlyphQuad]>,
    bounds: Rect,
    policy: CoveragePolicy,
    gradient: Option<Arc<GlyphGradient>>,
  },
  /// A paint shader's quad over `rect`, its params and inputs shared with
  /// the recorder's caller.
  DrawShader {
    rect: Rect,
    shader: PaintShader,
    params: Arc<[ShaderParam]>,
    inputs: Arc<[ShaderInput]>,
  },
  /// The nested list by reference, faded as a group by `opacity`.
  DrawList {
    list: DisplayList,
    opacity: f32,
  },
}
