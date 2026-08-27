use crate::gpu::NodeShader;
use crate::impellers::{DisplayListBuilder, Matrix};
use crate::rendertree::hit::{HitContext, Hittable};
use crate::rendertree::Damage;
use crate::rendertree::{Bounded, BuildContext, Buildable, Element, ElementKind, Point, Rect, Size, Vector};
use std::cell::Cell;
use taffy::{FlexDirection, Style};

// Memoized transform for one layout size, holding both the paint matrix and its
// (lazily fallible) inverse for hit testing. The pointer hit path is recomputed
// every animation frame, so static Views on that path would otherwise recompose
// and re-invert their matrix each frame for no reason.
#[derive(Clone, Copy, Debug)]
struct TransformCache {
  size: Size,
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
  // Per-axis scale (anisotropic scaling, e.g. a horizontal squash for a
  // card-flip animation). A uniform JS `scale` is expanded to both axes in the
  // plugin layer; the rendertree only knows the two axes.
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
  // Transform-space translation (CSS `translate`), applied after the rotation
  // center is restored so it shifts in screen space. Not the layout position.
  pub translate: Option<Vector>,
  // Transform origin, one independent field per axis (CSS `transform-origin`).
  // None on an axis falls back to that axis's box center - except on a
  // detached view, which has no box: there the unset origin is the view's
  // local (0,0), the point all detached geometry is authored against (an
  // inherited-box center would move the pivot with the window size). See
  // OriginCoord for the Px/Fraction split.
  pub origin_x: Option<OriginCoord>,
  pub origin_y: Option<OriginCoord>,
  // Scroll offset applied to children, in box pixels, between the overflow
  // clip and any design-size fit (content slides under the fixed clip, one scroll
  // pixel per box pixel regardless of fit scale). Positive values shift
  // content leftward/upward (web convention: positive scrollX means scrolled
  // "into" the content from the left).
  pub scroll: Option<Vector>,
  // Group opacity in 0..1 (None = opaque): children are composited together,
  // then faded as a whole. Not part of the matrix; applied at composite time
  // on boundaries, via a save_layer at record time otherwise.
  pub opacity: Option<f32>,
  // Corner radii [top-left, top-right, bottom-right, bottom-left] for the
  // clip applied when overflow is non-visible. None clips to a plain rect.
  pub clip_radius: Option<[f32; 4]>,
  // Design-space size for the children: everything under the view - layout,
  // paint, hit testing - happens in this w x h coordinate space, which is
  // uniformly scaled to fit and centered in the element's box (SVG's default
  // preserveAspectRatio, generalized). From the outside the view sizes like a
  // replaced element whose intrinsic size is the design size
  // (LayoutContext::design_size_layout). The fit is composed innermost, so the
  // user transform props still operate in box space.
  pub design_size: Option<Size>,
  // The shader declared on this view's snapshot boundary (see
  // composite::snapshot_node): one pass over the rasterized subtree,
  // composited in its place. Runs only with repaintBoundary="snapshot";
  // composite warns otherwise.
  pub shader: Option<NodeShader>,
  // A shader write since the last composite: the pass re-runs even when the
  // snapshot content is untouched (the params path). Interior-mutable
  // because the composite walk consumes it through a shared tree reference.
  shader_dirty: Cell<bool>,
  // True for a d-view (set by `Element::no_layout`): switches the unset-origin
  // fallback in `resolve_center` from box center to local (0,0). The size an
  // origin resolves against is the INHERITED box for a detached view, whose
  // center is window-size-dependent - never a sane default pivot.
  pub(crate) detached: bool,
  // Memoized transform; invalidated by the setters when a transform prop
  // changes, and recomputed by `transform` when the layout size differs.
  cache: Cell<Option<TransformCache>>,
}

impl View {
  fn resolve_translate(&self) -> Vector {
    self.translate.unwrap_or_default()
  }

  pub(crate) fn resolve_center(&self, size: Size) -> Point {
    // An explicit origin always resolves against `size` (the layout box, or
    // the inherited box for a detached view - so pct() origins on a d-view
    // still track that box, documented). Only the unset default differs.
    let (fx, fy) = if self.detached { (0.0, 0.0) } else { (size.width / 2.0, size.height / 2.0) };
    let x = self.origin_x.map_or(fx, |c| c.resolve(size.width));
    let y = self.origin_y.map_or(fy, |c| c.resolve(size.height));
    Point::new(x, y)
  }

  // Returns the memoized transform for `size`, recomposing (and re-inverting)
  // only on a cache miss: either the first call or a layout-size change. The
  // setters clear the cache when a transform prop changes.
  fn transform(&self, size: Size) -> TransformCache {
    if let Some(c) = self.cache.get() {
      if c.size == size {
        return c;
      }
    }
    let matrix = self.compose(size);
    let entry = TransformCache { size, matrix, inverse: matrix.inverse() };
    self.cache.set(Some(entry));
    entry
  }

  // The design-size fit: design space scaled-to-fit and centered in `size`. None
  // without a (non-degenerate) design_size. Kept separate from the user chain
  // because the two hoist differently: the fit belongs to the CONTENT (it is
  // recorded into boundary caches and captures), the user chain to the box
  // (hoisted to composite time). See composite::hoisted_matrix.
  pub(crate) fn fit_matrix(&self, size: Size) -> Option<Matrix> {
    let vb = self.design_size?;
    let s = self.fit_scale(size)?;
    let tx = (size.width - vb.width * s) / 2.0;
    let ty = (size.height - vb.height * s) / 2.0;
    Some(Matrix::new_2d(s, 0.0, 0.0, s, tx, ty))
  }

  // The design size the children live in: the design_size when set and
  // non-degenerate. Layout's inner root (LayoutContext::design_size_layout) and
  // the fit both come from here, so they cannot disagree.
  pub(crate) fn design_space(&self) -> Option<Size> {
    self.design_size.filter(|vb| vb.width > 0.0 && vb.height > 0.0)
  }

  // The uniform design-size fit scale `box / design`; None without a
  // (non-degenerate) design_size.
  pub(crate) fn fit_scale(&self, size: Size) -> Option<f32> {
    let vb = self.design_space()?;
    Some((size.width / vb.width).min(size.height / vb.height))
  }

  // The scroll offset expressed in the children's frame. Scroll means box
  // pixels on every path (it pairs with the box-space overflow clip -
  // okf/backlog/overflow-viewbox-clip.md); children of a design-size view live in
  // design space, so the offset divides by the fit scale there.
  pub(crate) fn content_scroll(&self, size: Size) -> Vector {
    let s = self.scroll.unwrap_or_default();
    match self.fit_scale(size) {
      Some(scale) => s / scale,
      None => s,
    }
  }

  // The full paint transform: the design-size fit (design -> box space), then the
  // user chain. Points apply left-first under euclid's `then`.
  fn compose(&self, size: Size) -> Matrix {
    let user = self.compose_user(size);
    match self.fit_matrix(size) {
      Some(fit) => fit.then(&user),
      None => user,
    }
  }

  // Composes the user transform chain around the rotation center - everything
  // except the design-size fit. The same (fit-composed) matrix drives both painting
  // and (inverted) hit testing, so the forward and inverse can never drift
  // apart. euclid's `then` applies the left transform first, so the chain reads
  // in point-application order.
  //
  // For the pure-2D case (no rotate_x/rotate_y/perspective) this reduces exactly
  // to the previous translate/scale/rotate op sequence.
  fn compose_user(&self, size: Size) -> Matrix {
    let p = self.resolve_translate();
    let c = self.resolve_center(size);

    // Move the rotation center to the origin.
    let mut m = Matrix::translation(-c.x, -c.y, 0.0);

    // Screen-clockwise z-rotation for a positive angle, matching the old
    // builder.rotate path (y-down space).
    if let Some(rot) = self.rotate {
      let (s, co) = rot.sin_cos();
      m = m.then(&Matrix::new_2d(co, s, -s, co, 0.0, 0.0));
    }

    if self.scale_x.is_some() || self.scale_y.is_some() {
      m = m.then(&Matrix::scale(self.scale_x.unwrap_or(1.0), self.scale_y.unwrap_or(1.0), 1.0));
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

  // Current full paint matrix for `size` (fit + user chain). The bounding-box
  // ancestor walk uses this to map child corners (which live in design space
  // under a design size) up into the parent frame.
  pub(crate) fn paint_matrix(&self, size: Size) -> Matrix {
    self.transform(size).matrix
  }

  // The matrix the view's own BOX transforms by: the user chain without the
  // design-size fit (the fit maps children into the box; it never moves the box
  // itself). Composite hoists this around cached content, and bounding-box
  // composition applies it to the view's own corners. Uncached: callers are
  // per-event or per-composite, not per-node-per-frame.
  pub(crate) fn box_matrix(&self, size: Size) -> Matrix {
    self.compose_user(size)
  }

  // True when the paint matrix does more than translate: bounding-box
  // composition must then walk corners through the matrix instead of taking
  // the cheap translation-only path.
  pub(crate) fn needs_matrix(&self) -> bool {
    self.rotate.is_some()
      || self.scale_x.is_some()
      || self.scale_y.is_some()
      || self.rotate_x.is_some()
      || self.rotate_y.is_some()
      || self.perspective.is_some()
      || self.design_size.is_some()
  }
}

impl Buildable for View {
  // A View paints no content of its own, and its matrices are applied by the
  // compositor (composite::own_matrix + the recorded fit), which splits the
  // user chain from the design-size fit around the overflow clip and scroll - a
  // single composed transform here could not keep those in box space.
  fn build<'a>(&'a self, _ctx: &mut BuildContext<'a>, _builder: &mut DisplayListBuilder) {}
}

impl Bounded for View {
  fn local_bounds(&self, fallback: Size) -> Rect {
    let p = self.translate.unwrap_or_default();
    Rect::new(p.to_point(), fallback)
  }
}

impl Hittable for View {
  fn transform_to_local(&self, point: Point, ctx: &HitContext) -> Point {
    // A point guaranteed to fail the bounds/clip checks, used when the transform
    // collapses (e.g. scaleX = 0 mid-flip): the element is then a clean miss.
    let miss = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);

    let Some(inv) = self.transform(ctx.size).inverse else {
      return miss;
    };

    // Apply the inverse on the local z = 0 plane (the content plane) with the
    // homogeneous divide. Exact for affine transforms; an approximation under
    // perspective, which is acceptable here since a flipped face carries no tap
    // target of its own. euclid returns None for w <= 0 (behind the eye), which
    // is a clean miss too.
    inv.transform_point2d(point).unwrap_or(miss)
  }
}

impl View {
  // Matrix props (translate, origin, rotate, scale, 3D) invalidate the memoized
  // matrix and report Damage::Compose: the View's own cached content stays
  // valid because composite applies the current matrix around it. Scroll
  // reports Damage::Scroll: a Recording cache survives (offset applied at
  // composite time), a Snapshot texture cannot (scrolled-out pixels are not
  // in it). clip_radius is baked into recorded content, so it reports Paint.
  //
  // Every setter takes an Option: None resets the prop to its unset default
  // (a cleared JS binding), which the plugin could not express with any
  // concrete value.
  pub fn set_rotate(&mut self, v: Option<f32>) -> Damage {
    self.rotate = v;
    self.invalidate();
    Damage::Compose
  }
  pub fn set_scale_x(&mut self, v: Option<f32>) -> Damage {
    self.scale_x = v;
    self.invalidate();
    Damage::Compose
  }
  pub fn set_scale_y(&mut self, v: Option<f32>) -> Damage {
    self.scale_y = v;
    self.invalidate();
    Damage::Compose
  }
  pub fn set_rotate_x(&mut self, v: Option<f32>) -> Damage {
    self.rotate_x = v;
    self.invalidate();
    Damage::Compose
  }
  pub fn set_rotate_y(&mut self, v: Option<f32>) -> Damage {
    self.rotate_y = v;
    self.invalidate();
    Damage::Compose
  }
  pub fn set_perspective(&mut self, v: Option<f32>) -> Damage {
    self.perspective = v;
    self.invalidate();
    Damage::Compose
  }
  // x/y reset per component; a translate that never existed stays None.
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    match (v, &mut self.translate) {
      (Some(v), t) => t.get_or_insert_with(Vector::default).x = v,
      (None, Some(t)) => t.x = 0.0,
      (None, None) => {}
    }
    self.invalidate();
    Damage::Compose
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    match (v, &mut self.translate) {
      (Some(v), t) => t.get_or_insert_with(Vector::default).y = v,
      (None, Some(t)) => t.y = 0.0,
      (None, None) => {}
    }
    self.invalidate();
    Damage::Compose
  }
  pub fn set_origin_x(&mut self, x: Option<OriginCoord>) -> Damage {
    self.origin_x = x;
    self.invalidate();
    Damage::Compose
  }
  pub fn set_origin_y(&mut self, y: Option<OriginCoord>) -> Damage {
    self.origin_y = y;
    self.invalidate();
    Damage::Compose
  }
  // Not a matrix prop (the memoized transform stays valid), but the same
  // damage class: both boundary modes apply the current opacity around their
  // cached content at composite time, and for a non-boundary View the baked
  // save_layer lives in the enclosing boundary's recording, which Compose's
  // parent-up invalidation clears.
  pub fn set_opacity(&mut self, v: Option<f32>) -> Damage {
    self.opacity = v.map(|v| v.clamp(0.0, 1.0));
    Damage::Compose
  }
  pub fn set_scroll_x(&mut self, v: Option<f32>) -> Damage {
    match (v, &mut self.scroll) {
      (Some(v), s) => s.get_or_insert_with(Vector::default).x = v,
      (None, Some(s)) => s.x = 0.0,
      (None, None) => {}
    }
    Damage::Scroll
  }
  pub fn set_scroll_y(&mut self, v: Option<f32>) -> Damage {
    match (v, &mut self.scroll) {
      (Some(v), s) => s.get_or_insert_with(Vector::default).y = v,
      (None, Some(s)) => s.y = 0.0,
      (None, None) => {}
    }
    Damage::Scroll
  }
  pub fn set_clip_radius(&mut self, radius: Option<[f32; 4]>) -> Damage {
    self.clip_radius = radius;
    Damage::Paint
  }
  /// Declare or clear the boundary shader: composite-time state, hence
  /// Damage::Compose - the snapshot texture itself stays valid (only the
  /// pass output changes; composite re-runs the pass off the dirty flag),
  /// while ancestor recordings hold the old composited quad and must
  /// repaint. Requires repaintBoundary="snapshot"; enforced with a
  /// composite-time warning rather than a throw here, because prop
  /// application order would make a set-time check misfire on elements that
  /// set `shader` before `repaintBoundary`.
  pub fn set_shader(&mut self, shader: Option<NodeShader>) -> Damage {
    self.shader = shader;
    self.shader_dirty.set(true);
    Damage::Compose
  }

  /// Flag the pass to re-run without a declaration change: the shader's
  /// extra texture inputs are sampled at pass time, so when one's CONTENT
  /// changes (see `RenderTree::texture_content_changed`) a rerun over the
  /// still-valid snapshot is all that is stale - the same
  /// dirty-flag-plus-Compose shape as a params write through `set_shader`.
  pub(crate) fn mark_shader_dirty(&self) {
    self.shader_dirty.set(true);
  }

  /// Take the pending-shader-write flag; composite consumes it at the paint
  /// walk (running the pass, or warning when the boundary is missing).
  pub(crate) fn take_shader_dirty(&self) -> bool {
    self.shader_dirty.replace(false)
  }

  // Layout, not Paint: the design size is the children's layout space and the
  // view's intrinsic size (LayoutContext::design_size_layout), so changing it
  // re-solves the subtree. Layout damage re-records the content as well,
  // which the fit is part of (recorded into boundary caches and snapshot
  // textures, unlike the hoisted user chain).
  pub fn set_design_size(&mut self, size: Option<(f32, f32)>) -> Damage {
    self.design_size = size.map(|(w, h)| Size::new(w, h));
    self.invalidate();
    Damage::Layout
  }

  // The style a layout view starts with; layout-prop resets restore fields
  // from here (a view's block axis is the column, unlike taffy's row).
  pub fn initial_style() -> Style {
    Style { flex_direction: FlexDirection::Column, ..Style::default() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::View(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::View(self))
  }
}
