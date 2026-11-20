//! What a list executes: anvil's own ops, resolved at record time (colors
//! premultiplied, a paint reduced to what anvil draws, a gradient source
//! resolved to its rows and ramp), shared behind an Arc so a retained list
//! is recorded once and replayed by reference.

use super::mesh::MeshStyle;
use super::path::Path;
use super::record::DisplayList;
use super::{PaintShader, ShaderInput, Texture};
use crate::TileMode;
use crate::{
  BlendMode, ClipOperation, Color, ColorMatrix, Matrix, Point, Rect, ShaderParam, StrokeCap, TextureSampling,
};
use crate::{CoveragePolicy, GlyphQuad, RampGradient};
use euclid::default::Transform2D;
use std::sync::{Arc, OnceLock};

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
/// the draw's space by an invertible 2D affine map (`to_texel` is that
/// map's inverse, local to texel, resolved at record time), sampled
/// `sampling`, clamped at the edges.
#[derive(Clone)]
pub(super) struct Image {
  pub texture: Texture,
  pub sampling: TextureSampling,
  pub to_texel: Transform2D<f32>,
}

/// What fills a shape's coverage in place of its color: an image's texels,
/// or a gradient's color (resolved at record time, `record::Paint`), faded
/// by the op's alpha either way.
#[derive(Clone)]
pub(super) enum Source {
  Image(Image),
  Gradient(Arc<RampGradient>),
}

/// A gaussian blur over a layer's pixels, as a paint's image filter or a
/// backdrop layer's filter gives it: the standard deviations along the
/// layer's local x and y axes, in local units, and how the blur reads past
/// the content's edge: `Clamp` extends the content's edge color outward,
/// `Decal` reads transparent there; either way a layer's output grows by the
/// blur's reach (walk.rs `close_layer`). The recording maps `Mirror` and
/// `Repeat` to `Clamp`, warned once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Blur {
  pub sigma_x: f32,
  pub sigma_y: f32,
  pub tile: TileMode,
}

/// One executable op of a list, in recording order.
pub(super) enum Op {
  Save,
  /// A save whose content draws offscreen and composites as one, faded by
  /// `alpha` (straight, 0..1): group opacity; its pixels run through `blur`
  /// before the composite and through the color `matrix` in it. The
  /// matching `Restore` closes it (the walk knows what it pops).
  SaveLayer {
    alpha: f32,
    matrix: Option<ColorMatrix>,
    blur: Option<Blur>,
  },
  /// A save whose layer starts from the target's pixels under `bounds`
  /// (local, within the clip in effect), run through `blur`; the content
  /// draws on top, and the restore composites the layer back faded by
  /// `alpha` through the color `matrix`, source-over.
  SaveBackdropLayer {
    bounds: Rect,
    alpha: f32,
    matrix: Option<ColorMatrix>,
    blur: Option<Blur>,
  },
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
  /// A clip to `rect` combined by `op` with the clips in effect.
  ClipRect {
    rect: Rect,
    op: ClipOperation,
  },
  /// As `ClipRect`, with the corner `radii` in the rect's own space,
  /// clamped. `path` is the rounded rect as a path, built by the walk at
  /// the op's first stencil clip and kept with the op, so a retained list
  /// keeps the path, its mesh cache and the mesh's GPU buffers.
  ClipRoundedRect {
    rect: Rect,
    radii: Radii,
    op: ClipOperation,
    path: OnceLock<Path>,
  },
  /// A clip to the oval in `bounds`, combined by `op`; `path` as for
  /// `ClipRoundedRect` (an oval always clips through the stencil).
  ClipOval {
    bounds: Rect,
    op: ClipOperation,
    path: OnceLock<Path>,
  },
  /// `stroke` None fills; Some(width) strokes centered on the edge, with
  /// miter joins (sharp corners) where a radius is zero. `radii` all zero
  /// is a plain rect. `source` Some fills the coverage with the image's
  /// texels or the gradient's color, faded by `color`'s alpha. `blend` is
  /// the paint's, as on every op that takes a paint, and so is `matrix`,
  /// the paint's color filter.
  DrawRect {
    rect: Rect,
    radii: Radii,
    color: Premultiplied,
    stroke: Option<f32>,
    source: Option<Source>,
    blend: BlendMode,
    matrix: Option<ColorMatrix>,
  },
  /// `stroke` None fills; Some(width) strokes centered on the ellipse.
  /// `source` as for `DrawRect`.
  DrawOval {
    bounds: Rect,
    color: Premultiplied,
    stroke: Option<f32>,
    source: Option<Source>,
    blend: BlendMode,
    matrix: Option<ColorMatrix>,
  },
  /// A segment stroked `width` wide, centered on it, ended by `cap` at
  /// both ends. A width of zero or below draws nothing (the walk skips it).
  /// `gradient` Some fills the coverage with the gradient's color, faded
  /// by `color`'s alpha.
  DrawLine {
    from: Point,
    to: Point,
    color: Premultiplied,
    width: f32,
    cap: StrokeCap,
    gradient: Option<Arc<RampGradient>>,
    blend: BlendMode,
    matrix: Option<ColorMatrix>,
  },
  /// A path's mesh under `style`, filled with `color`, or with `source`
  /// faded by `color`'s alpha. One op per style: a stroke-and-fill records
  /// a fill op and then a stroke op, so the stroke covers the fill as
  /// Skia's does.
  DrawPath {
    path: Path,
    style: MeshStyle,
    color: Premultiplied,
    source: Option<Source>,
    blend: BlendMode,
    matrix: Option<ColorMatrix>,
  },
  /// `alpha` is the paint's alpha (1 without a paint): a texture's opacity
  /// is per op, not a group. `blend` is the paint's, source-over without
  /// one; `matrix` its color filter, none without one.
  DrawTexture {
    texture: Texture,
    src: Rect,
    dst: Rect,
    sampling: TextureSampling,
    alpha: f32,
    blend: BlendMode,
    matrix: Option<ColorMatrix>,
  },
  /// A glyph run: `quads` over `atlas`, whose red channel is coverage,
  /// under `policy`, colored by `color` or along `gradient`. `bounds` is the
  /// quads' union in the op's space, so the walk rejects a whole run that
  /// falls outside the clip before it looks at a glyph.
  DrawGlyphs {
    atlas: Texture,
    quads: Arc<[GlyphQuad]>,
    bounds: Rect,
    /// The run's color, straight alpha (see `draw_glyphs`).
    color: [f32; 4],
    policy: CoveragePolicy,
    gradient: Option<Arc<RampGradient>>,
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
