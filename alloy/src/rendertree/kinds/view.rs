use crate::impellers::{DisplayListBuilder, Matrix};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BoundingBox, BuildContext, Buildable, Element, ElementKind, WH, XY};
use std::cell::Cell;
use taffy::{FlexDirection, Size, Style};

// Memoized transform for one layout size, holding both the paint matrix and its
// (lazily fallible) inverse for hit testing. The pointer hit path is recomputed
// every animation frame, so static Views on that path would otherwise recompose
// and re-invert their matrix each frame for no reason.
#[derive(Clone, Copy, Debug)]
struct TransformCache {
  size: WH,
  matrix: Matrix,
  inverse: Option<Matrix>,
}

// One axis of the transform origin. `Px` is an absolute offset; `Fraction` is a
// share of the layout extent (0.5 = center), resolved against the live size at
// compose time so a percentage origin tracks resizes with no external wiring.
#[derive(Clone, Copy, Debug)]
pub enum OriginCoord {
  Px(f32),
  Fraction(f32),
}

impl OriginCoord {
  fn resolve(self, extent: f32) -> f32 {
    match self {
      OriginCoord::Px(v) => v,
      OriginCoord::Fraction(f) => f * extent,
    }
  }
}

#[derive(Clone, Debug, Default)]
pub struct View {
  pub rotate: Option<f32>,
  // Uniform scale; scale_x/scale_y override it per axis (anisotropic scaling,
  // e.g. a horizontal squash for a card-flip animation).
  pub scale: Option<f32>,
  pub scale_x: Option<f32>,
  pub scale_y: Option<f32>,
  // 3D rotation about the horizontal (X) axis, in radians (a top/bottom tilt).
  // Like rotate_y, reads as 3D only with a `perspective` set.
  pub rotate_x: Option<f32>,
  // 3D rotation about the vertical (Y) axis, in radians, for a card-flip. With
  // a `perspective` set it reads as a real flip; without one it is orthographic.
  pub rotate_y: Option<f32>,
  // Perspective viewing distance in pixels (CSS `perspective`). Larger is a
  // shallower, less dramatic 3D effect. Applied around the rotation center.
  pub perspective: Option<f32>,
  pub pos: Option<XY>,
  // Transform origin per axis (CSS `transform-origin`). None on either axis
  // falls back to the box center. See OriginCoord for the Px/Fraction split.
  pub origin: Option<(OriginCoord, OriginCoord)>,
  // Scroll offset applied to children at build time, after the clip is set.
  // Positive values shift content leftward/upward (web convention: positive
  // scrollX means scrolled "into" the content from the left).
  pub scroll: Option<XY>,
  // Group opacity in 0..1 (None = opaque): children are composited together,
  // then faded as a whole. Not part of the matrix; applied at composite time
  // on boundaries, via a save_layer at record time otherwise.
  pub opacity: Option<f32>,
  // Corner radii [top-left, top-right, bottom-right, bottom-left] for the
  // clip applied when overflow is non-visible. None clips to a plain rect.
  pub clip_radius: Option<[f32; 4]>,
  // Memoized transform; invalidated by the setters when a transform prop
  // changes, and recomputed by `transform` when the layout size differs.
  cache: Cell<Option<TransformCache>>,
}

impl View {
  fn resolve_pos(&self) -> XY {
    self.pos.unwrap_or_default()
  }

  fn resolve_center(&self, size: WH) -> XY {
    match self.origin {
      Some((x, y)) => XY::new(x.resolve(size.w), y.resolve(size.h)),
      None => XY::new(size.w / 2.0, size.h / 2.0),
    }
  }

  // Returns the memoized transform for `size`, recomposing (and re-inverting)
  // only on a cache miss: either the first call or a layout-size change. The
  // setters clear the cache when a transform prop changes.
  fn transform(&self, size: WH) -> TransformCache {
    if let Some(c) = self.cache.get() {
      if c.size.w == size.w && c.size.h == size.h {
        return c;
      }
    }
    let matrix = self.compose(size);
    let entry = TransformCache { size, matrix, inverse: matrix.inverse() };
    self.cache.set(Some(entry));
    entry
  }

  // Composes the full paint transform around the rotation center. The same
  // matrix drives both painting and (inverted) hit testing, so the forward and
  // inverse can never drift apart. euclid's `then` applies the left transform
  // first, so the chain reads in point-application order.
  //
  // For the pure-2D case (no rotate_x/rotate_y/perspective) this reduces exactly
  // to the previous translate/scale/rotate op sequence.
  fn compose(&self, size: WH) -> Matrix {
    let p = self.resolve_pos();
    let c = self.resolve_center(size);

    // Move the rotation center to the origin.
    let mut m = Matrix::translation(-c.x, -c.y, 0.0);

    // Screen-clockwise z-rotation for a positive angle, matching the old
    // builder.rotate path (y-down space).
    if let Some(rot) = self.rotate {
      let (s, co) = rot.sin_cos();
      m = m.then(&Matrix::new_2d(co, s, -s, co, 0.0, 0.0));
    }

    let sx = self.scale_x.or(self.scale);
    let sy = self.scale_y.or(self.scale);
    if sx.is_some() || sy.is_some() {
      m = m.then(&Matrix::scale(sx.unwrap_or(1.0), sy.unwrap_or(1.0), 1.0));
    }

    // 3D rotations about the centered card (X = top/bottom tilt, Y = card-flip),
    // then the perspective foreshorten.
    if let Some(rotx) = self.rotate_x {
      let (s, co) = rotx.sin_cos();
      #[rustfmt::skip]
      let rx = Matrix::new(
        1.0, 0.0, 0.0, 0.0,
        0.0, co,  s,   0.0,
        0.0, -s,  co,  0.0,
        0.0, 0.0, 0.0, 1.0,
      );
      m = m.then(&rx);
    }
    if let Some(roty) = self.rotate_y {
      let (s, co) = roty.sin_cos();
      #[rustfmt::skip]
      let ry = Matrix::new(
        co,  0.0, -s,  0.0,
        0.0, 1.0, 0.0, 0.0,
        s,   0.0, co,  0.0,
        0.0, 0.0, 0.0, 1.0,
      );
      m = m.then(&ry);
    }
    if let Some(d) = self.perspective {
      if d != 0.0 {
        m = m.then(&Matrix::perspective(d));
      }
    }

    // Restore the center and apply the position offset. Both are pure
    // translations, so they fold into one; applied after perspective they shift
    // in screen space (post-divide), which is what we want.
    m.then(&Matrix::translation(c.x + p.x, c.y + p.y, 0.0))
  }

  // Clears the memoized transform; called by the setters on a prop change.
  fn invalidate(&self) {
    self.cache.set(None);
  }

  // Current paint matrix for `size`. Composite uses this to hoist a boundary
  // View's transform out of its cached content (see composite::hoisted_matrix).
  pub(crate) fn paint_matrix(&self, size: WH) -> Matrix {
    self.transform(size).matrix
  }
}

impl Buildable for View {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    builder.transform(&self.transform(ctx.size).matrix);
  }
}

impl Bounded for View {
  fn local_bounds(&self, fallback: Size<f32>) -> BoundingBox {
    let p = self.pos.unwrap_or_default();
    BoundingBox { x: p.x, y: p.y, width: fallback.width, height: fallback.height }
  }
}

impl Hittable for View {
  fn transform_to_local(&self, point: XY, ctx: &HitContext) -> XY {
    // A point guaranteed to fail the bounds/clip checks, used when the transform
    // collapses (e.g. scaleX = 0 mid-flip): the element is then a clean miss.
    let miss = XY::new(f32::NEG_INFINITY, f32::NEG_INFINITY);

    let Some(inv) = self.transform(ctx.size).inverse else {
      return miss;
    };

    // Apply the inverse to the screen point assuming local z = 0 (the content
    // plane), with the homogeneous divide. Exact for affine transforms; an
    // approximation under perspective, which is acceptable here since a flipped
    // face carries no tap target of its own.
    let x = point.x * inv.m11 + point.y * inv.m21 + inv.m41;
    let y = point.x * inv.m12 + point.y * inv.m22 + inv.m42;
    let w = point.x * inv.m14 + point.y * inv.m24 + inv.m44;
    if w == 0.0 {
      return miss;
    }
    XY::new(x / w, y / w)
  }
}

impl View {
  // Matrix props (pos, origin, rotate, scale, 3D) invalidate the memoized
  // matrix and report Damage::Transform: the View's own cached content stays
  // valid because composite applies the current matrix around it. Scroll
  // reports Damage::Scroll: a Recording cache survives (offset applied at
  // composite time), a Snapshot texture cannot (scrolled-out pixels are not
  // in it). clip_radius is baked into recorded content, so it reports Paint.
  pub fn set_rotate(&mut self, v: f32) -> Damage {
    self.rotate = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_scale(&mut self, v: f32) -> Damage {
    self.scale = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_scale_x(&mut self, v: f32) -> Damage {
    self.scale_x = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_scale_y(&mut self, v: f32) -> Damage {
    self.scale_y = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_rotate_x(&mut self, v: f32) -> Damage {
    self.rotate_x = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_rotate_y(&mut self, v: f32) -> Damage {
    self.rotate_y = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_perspective(&mut self, v: f32) -> Damage {
    self.perspective = Some(v);
    self.invalidate();
    Damage::Transform
  }
  pub fn set_x(&mut self, v: f32) -> Damage {
    self.pos.get_or_insert_with(XY::default).x = v;
    self.invalidate();
    Damage::Transform
  }
  pub fn set_y(&mut self, v: f32) -> Damage {
    self.pos.get_or_insert_with(XY::default).y = v;
    self.invalidate();
    Damage::Transform
  }
  pub fn set_origin(&mut self, x: OriginCoord, y: OriginCoord) -> Damage {
    self.origin = Some((x, y));
    self.invalidate();
    Damage::Transform
  }
  // Not a matrix prop (the memoized transform stays valid), but the same
  // damage class: both boundary modes apply the current opacity around their
  // cached content at composite time, and for a non-boundary View the baked
  // save_layer lives in the enclosing boundary's recording, which Transform's
  // parent-up invalidation clears.
  pub fn set_opacity(&mut self, v: f32) -> Damage {
    self.opacity = Some(v.clamp(0.0, 1.0));
    Damage::Transform
  }
  pub fn set_scroll_x(&mut self, v: f32) -> Damage {
    self.scroll.get_or_insert_with(XY::default).x = v;
    Damage::Scroll
  }
  pub fn set_scroll_y(&mut self, v: f32) -> Damage {
    self.scroll.get_or_insert_with(XY::default).y = v;
    Damage::Scroll
  }
  pub fn set_clip_radius(&mut self, radius: [f32; 4]) -> Damage {
    self.clip_radius = Some(radius);
    Damage::Paint
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::View(self), Style { flex_direction: FlexDirection::Column, ..Style::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::View(self))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn origin_defaults_to_center() {
    let v = View::default();
    let c = v.resolve_center(WH::new(200.0, 100.0));
    assert_eq!((c.x, c.y), (100.0, 50.0));
  }

  #[test]
  fn fraction_origin_tracks_size() {
    let mut v = View::default();
    v.set_origin(OriginCoord::Fraction(0.0), OriginCoord::Fraction(1.0));
    // Top-left on x, bottom on y, resolved against the live extent.
    let c = v.resolve_center(WH::new(200.0, 100.0));
    assert_eq!((c.x, c.y), (0.0, 100.0));
    // Same fractions, a different size: the pivot moves with the box.
    let c = v.resolve_center(WH::new(40.0, 60.0));
    assert_eq!((c.x, c.y), (0.0, 60.0));
  }

  #[test]
  fn pixel_origin_is_absolute() {
    let mut v = View::default();
    v.set_origin(OriginCoord::Px(20.0), OriginCoord::Px(30.0));
    let c = v.resolve_center(WH::new(200.0, 100.0));
    assert_eq!((c.x, c.y), (20.0, 30.0));
    // Unaffected by size, unlike a fraction.
    let c = v.resolve_center(WH::new(40.0, 60.0));
    assert_eq!((c.x, c.y), (20.0, 30.0));
  }
}
