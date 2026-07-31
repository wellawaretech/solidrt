//! Engine-free SVG parsing: a whole SVG document string in, a flat list of
//! plain draw commands out. usvg does the heavy lifting (CSS, transforms,
//! use/defs, gradients are normalized into a flat path tree); this module
//! flattens that tree further into per-path fill/stroke draws whose geometry
//! is baked into document coordinates. The host maps each draw onto its own
//! path primitive - nothing here names a renderer or scripting type.
//!
//! Limitations (documented, deferred): clipPath, masks, filters, patterns,
//! exact group opacity, and SVG <text> are not applied; radial gradient focal
//! point (fx/fy/fr) is ignored.

use usvg::tiny_skia_path::{PathSegment, Transform};

/// A parsed document: intrinsic size (viewBox/width-height) plus the flat
/// draw list in document coordinates.
pub struct SvgDocument {
  pub width: f32,
  pub height: f32,
  pub draws: Vec<SvgDraw>,
}

/// One resolved draw: absolute-coordinate path data plus the paint to draw it
/// with. A source path with both a fill and a stroke yields two draws (fill
/// first), so differing fill/stroke colors are preserved.
pub struct SvgDraw {
  /// SVG path data (`M`/`L`/`Q`/`C`/`Z`, absolute), with every group/element
  /// transform already applied.
  pub d: String,
  pub paint: SvgPaint,
  pub style: SvgDrawStyle,
  /// Set for fills only.
  pub fill_rule: Option<SvgFillRule>,
  /// Stroke params, set for strokes only. The width is pre-scaled by the
  /// baked transform's uniform scale factor so it matches the geometry.
  pub stroke_width: Option<f32>,
  pub stroke_cap: Option<SvgStrokeCap>,
  pub stroke_join: Option<SvgStrokeJoin>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SvgDrawStyle {
  Fill,
  Stroke,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SvgFillRule {
  NonZero,
  EvenOdd,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SvgStrokeCap {
  Butt,
  Round,
  Square,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SvgStrokeJoin {
  Miter,
  Round,
  Bevel,
}

/// Colors are packed `0xRRGGBBAA`. Gradient coordinates are in document space
/// (usvg resolves userSpaceOnUse/objectBoundingBox to absolute values);
/// `transform` maps them into the same space as the baked geometry, as an SVG
/// `matrix(a b c d e f)` sextet.
pub enum SvgPaint {
  Solid(u32),
  Linear { x0: f32, y0: f32, x1: f32, y1: f32, stops: Vec<SvgStop>, spread: SvgSpread, transform: [f32; 6] },
  Radial { cx: f32, cy: f32, r: f32, stops: Vec<SvgStop>, spread: SvgSpread, transform: [f32; 6] },
}

pub struct SvgStop {
  /// 0..1 along the gradient.
  pub offset: f32,
  /// Packed `0xRRGGBBAA`, with the owning fill/stroke opacity folded in.
  pub color: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SvgSpread {
  Pad,
  Reflect,
  Repeat,
}

/// Parses an SVG document string. `current_color` (packed `0xRRGGBBAA`, alpha
/// ignored) drives `currentColor` in the document via an injected stylesheet;
/// explicit fills/strokes still win.
pub fn parse(src: &str, current_color: Option<u32>) -> Result<SvgDocument, String> {
  let mut opt = usvg::Options::default();
  // Sandbox: usvg never makes network requests, and we additionally forbid all
  // external resource access. Both image href resolvers return None (no remote,
  // no local file, no embedded data-URI rasters) and there is no base directory
  // for relative references to resolve against.
  opt.resources_dir = None;
  opt.image_href_resolver =
    usvg::ImageHrefResolver { resolve_data: Box::new(|_, _, _| None), resolve_string: Box::new(|_, _| None) };
  if let Some(c) = current_color {
    opt.style_sheet = Some(format!("* {{ color: #{:06x} }}", c >> 8));
  }

  let tree = usvg::Tree::from_str(src, &opt).map_err(|err| format!("SVG parse failed: {err}"))?;

  let size = tree.size();
  let mut draws = Vec::new();
  collect(tree.root(), &mut draws);

  Ok(SvgDocument { width: size.width(), height: size.height(), draws })
}

fn collect(group: &usvg::Group, out: &mut Vec<SvgDraw>) {
  for node in group.children() {
    match node {
      usvg::Node::Group(g) => collect(g, out),
      usvg::Node::Path(p) => convert_path(p, out),
      // Images are sandboxed out (resolvers return None); text needs the
      // disabled font stack.
      usvg::Node::Image(_) | usvg::Node::Text(_) => {}
    }
  }
}

fn convert_path(path: &usvg::Path, out: &mut Vec<SvgDraw>) {
  let transform = path.abs_transform();

  if let Some(fill) = path.fill() {
    out.push(SvgDraw {
      d: build_d(path.data(), &transform),
      paint: resolve_paint(fill.paint(), fill.opacity(), &transform),
      style: SvgDrawStyle::Fill,
      fill_rule: Some(match fill.rule() {
        usvg::FillRule::NonZero => SvgFillRule::NonZero,
        usvg::FillRule::EvenOdd => SvgFillRule::EvenOdd,
      }),
      stroke_width: None,
      stroke_cap: None,
      stroke_join: None,
    });
  }

  if let Some(stroke) = path.stroke() {
    out.push(SvgDraw {
      d: build_d(path.data(), &transform),
      paint: resolve_paint(stroke.paint(), stroke.opacity(), &transform),
      style: SvgDrawStyle::Stroke,
      fill_rule: None,
      // The geometry is baked through `transform`, so the width scales by the
      // transform's area-preserving uniform factor to stay proportional (a
      // `scale(2)` group doubles the stroke, as a rasterizer would).
      stroke_width: Some(stroke.width().get() * uniform_scale(&transform)),
      stroke_cap: Some(match stroke.linecap() {
        usvg::LineCap::Butt => SvgStrokeCap::Butt,
        usvg::LineCap::Round => SvgStrokeCap::Round,
        usvg::LineCap::Square => SvgStrokeCap::Square,
      }),
      stroke_join: Some(match stroke.linejoin() {
        usvg::LineJoin::Round => SvgStrokeJoin::Round,
        usvg::LineJoin::Bevel => SvgStrokeJoin::Bevel,
        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => SvgStrokeJoin::Miter,
      }),
    });
  }
}

// Serializes the path with `t` applied to every point, so the emitted data is
// in document coordinates and the consumer needs no transform support.
fn build_d(data: &usvg::tiny_skia_path::Path, t: &Transform) -> String {
  let mut d = String::new();
  let map = |p: usvg::tiny_skia_path::Point| -> (f32, f32) {
    let mut p = p;
    t.map_point(&mut p);
    (p.x, p.y)
  };
  let mut push = |cmd: &str, points: &[(f32, f32)]| {
    if !d.is_empty() {
      d.push(' ');
    }
    d.push_str(cmd);
    for (x, y) in points {
      d.push_str(&format!(" {x} {y}"));
    }
  };
  for seg in data.segments() {
    match seg {
      PathSegment::MoveTo(p) => push("M", &[map(p)]),
      PathSegment::LineTo(p) => push("L", &[map(p)]),
      PathSegment::QuadTo(c, p) => push("Q", &[map(c), map(p)]),
      PathSegment::CubicTo(c1, c2, p) => push("C", &[map(c1), map(c2), map(p)]),
      PathSegment::Close => push("Z", &[]),
    }
  }
  d
}

// The uniform scale factor of an affine transform: sqrt of the absolute
// determinant, i.e. the length scale that preserves the transform's area
// change. Exact for uniform scales and rotations; the standard compromise for
// anisotropic ones (SVG strokes have a single width).
fn uniform_scale(t: &Transform) -> f32 {
  (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
}

// Resolves a usvg paint into plain data, folding the owning fill/stroke
// opacity into the color (solids) or every stop (gradients). Patterns are
// unsupported and fall back to mid-gray. `abs` is the path's absolute
// transform, which maps the gradient's coordinates into the same space as the
// baked geometry.
fn resolve_paint(paint: &usvg::Paint, opacity: usvg::Opacity, abs: &Transform) -> SvgPaint {
  let alpha = opacity.get();
  match paint {
    usvg::Paint::Color(c) => SvgPaint::Solid(pack(c.red, c.green, c.blue, alpha)),
    usvg::Paint::LinearGradient(grad) => SvgPaint::Linear {
      x0: grad.x1(),
      y0: grad.y1(),
      x1: grad.x2(),
      y1: grad.y2(),
      stops: convert_stops(grad.stops(), alpha),
      spread: convert_spread(grad.spread_method()),
      transform: sextet(&abs.pre_concat(grad.transform())),
    },
    usvg::Paint::RadialGradient(grad) => SvgPaint::Radial {
      cx: grad.cx(),
      cy: grad.cy(),
      r: grad.r().get(),
      stops: convert_stops(grad.stops(), alpha),
      spread: convert_spread(grad.spread_method()),
      transform: sextet(&abs.pre_concat(grad.transform())),
    },
    usvg::Paint::Pattern(_) => SvgPaint::Solid(pack(128, 128, 128, alpha)),
  }
}

fn pack(r: u8, g: u8, b: u8, alpha: f32) -> u32 {
  let a = (alpha * 255.0).round().clamp(0.0, 255.0) as u32;
  ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | a
}

fn convert_stops(stops: &[usvg::Stop], fill_alpha: f32) -> Vec<SvgStop> {
  stops
    .iter()
    .map(|s| {
      let c = s.color();
      SvgStop { offset: s.offset().get(), color: pack(c.red, c.green, c.blue, s.opacity().get() * fill_alpha) }
    })
    .collect()
}

fn convert_spread(spread: usvg::SpreadMethod) -> SvgSpread {
  match spread {
    usvg::SpreadMethod::Pad => SvgSpread::Pad,
    usvg::SpreadMethod::Reflect => SvgSpread::Reflect,
    usvg::SpreadMethod::Repeat => SvgSpread::Repeat,
  }
}

// tiny_skia affine (x' = sx*x + kx*y + tx, y' = ky*x + sy*y + ty) as the SVG
// matrix(a b c d e f) sextet: a=sx, b=ky, c=kx, d=sy, e=tx, f=ty.
fn sextet(t: &Transform) -> [f32; 6] {
  [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty]
}
