use super::{Gradient, GradientStop, GradientUnits, PaintState};
use crate::rendertree::{BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext};
use crate::impellers::{
  Color, DisplayListBuilder, DrawStyle, FillType, Matrix, Path as ImpPath, PathBuilder, Point, StrokeCap, StrokeJoin, TileMode,
};
use std::cell::RefCell;
use usvg::tiny_skia_path::{PathSegment, Transform};

// A single resolved draw: a built Impeller path in viewBox (user) coordinates,
// plus the solid paint to draw it with. A usvg path with both a fill and a
// stroke yields two of these, so differing fill/stroke colors are preserved.
struct DrawCmd {
  path: ImpPath,
  paint: PaintState,
}

struct Built {
  cmds: Vec<DrawCmd>,
  // Intrinsic size from the SVG viewBox/width-height; the draw geometry lives in
  // this coordinate space and is scaled into the layout box at paint time.
  intrinsic: (f32, f32),
}

// Renders a whole SVG document. usvg parses and normalizes the markup (CSS,
// transforms, use/defs, gradients, clips) into a flat tree of paths, which we
// flatten further into solid-fill/stroke draws. This is the document-level
// convenience layer above the single-shape `Path` primitive; it does not
// replace it (Path stays the low-level, hit-testable, reactively-driven shape).
//
// Limitations (documented, deferred): clipPath, masks, filters, patterns, exact
// group opacity, and SVG <text> are not applied; radial gradient focal point
// (fx/fy/fr) is ignored; hit-testing is bounding-box only.
pub struct Svg {
  pub src: String,
  // Drives `currentColor` (injected as a usvg stylesheet). Explicit fills/strokes
  // in the document still win; this only colors shapes that defer to currentColor.
  pub color: Option<Color>,
  built: RefCell<Option<Built>>,
}

impl Default for Svg {
  fn default() -> Self {
    Self { src: String::new(), color: None, built: RefCell::new(None) }
  }
}

impl Svg {
  fn ensure_built(&self) {
    if self.built.borrow().is_some() {
      return;
    }
    if self.src.is_empty() {
      return;
    }

    let mut opt = usvg::Options::default();
    // Sandbox: usvg never makes network requests, and we additionally forbid all
    // external resource access. Both image href resolvers return None (no remote,
    // no local file, no embedded data-URI rasters) and there is no base directory
    // for relative references to resolve against.
    opt.resources_dir = None;
    opt.image_href_resolver = usvg::ImageHrefResolver {
      resolve_data: Box::new(|_, _, _| None),
      resolve_string: Box::new(|_, _| None),
    };
    if let Some(c) = self.color {
      opt.style_sheet = Some(format!("* {{ color: {} }}", color_to_hex(c)));
    }

    let tree = match usvg::Tree::from_str(&self.src, &opt) {
      Ok(tree) => tree,
      Err(err) => {
        log::warn!("svg parse failed: {err}");
        return;
      }
    };

    let size = tree.size();
    let mut cmds = Vec::new();
    collect(tree.root(), &mut cmds);

    *self.built.borrow_mut() = Some(Built { cmds, intrinsic: (size.width(), size.height()) });
  }

  pub fn invalidate(&self) {
    *self.built.borrow_mut() = None;
  }

  // `src` changes both geometry and intrinsic size, so it invalidates layout.
  pub fn set_src(&mut self, src: String) -> bool {
    self.src = src;
    self.invalidate();
    true
  }

  // currentColor only affects paint, never size/layout.
  pub fn set_color(&mut self, color: Color) -> bool {
    self.color = Some(color);
    self.invalidate();
    false
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Svg(self), taffy::Style { display: taffy::Display::Block, ..Default::default() })
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Svg(self))
  }
}

impl Buildable for Svg {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    self.ensure_built();
    let built = self.built.borrow();
    let Some(built) = built.as_ref() else { return };
    let (iw, ih) = built.intrinsic;
    if iw <= 0.0 || ih <= 0.0 {
      return;
    }

    // Uniform scale-to-fit and center (SVG default preserveAspectRatio).
    let scale = (ctx.size.w / iw).min(ctx.size.h / ih);
    let tx = (ctx.size.w - iw * scale) / 2.0;
    let ty = (ctx.size.h - ih * scale) / 2.0;

    builder.save();
    builder.translate(tx, ty);
    builder.scale(scale, scale);
    for cmd in &built.cmds {
      let paint = cmd.paint.to_paint();
      builder.draw_path(&cmd.path, &paint);
    }
    builder.restore();
  }
}

impl Measurable for Svg {
  fn measure(&self, ctx: &MeasureContext) -> taffy::Size<f32> {
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return taffy::Size { width: w, height: h };
    }
    self.ensure_built();
    let built = self.built.borrow();
    let Some(built) = built.as_ref() else {
      return taffy::Size::ZERO;
    };
    let (iw, ih) = built.intrinsic;
    taffy::Size { width: ctx.known.width.unwrap_or(iw), height: ctx.known.height.unwrap_or(ih) }
  }
}

fn collect(group: &usvg::Group, out: &mut Vec<DrawCmd>) {
  for node in group.children() {
    match node {
      usvg::Node::Group(g) => collect(g, out),
      usvg::Node::Path(p) => convert_path(p, out),
      // Images are sandboxed out (resolvers return None); text needs the disabled
      // font stack. Both are stage-1 gaps.
      usvg::Node::Image(_) | usvg::Node::Text(_) => {}
    }
  }
}

fn convert_path(path: &usvg::Path, out: &mut Vec<DrawCmd>) {
  let transform = path.abs_transform();

  if let Some(fill) = path.fill() {
    let rule = match fill.rule() {
      usvg::FillRule::EvenOdd => FillType::Odd,
      usvg::FillRule::NonZero => FillType::NonZero,
    };
    let imp = build_imp_path(path.data(), &transform, rule);
    let (color, gradient) = resolve_paint(fill.paint(), fill.opacity(), &transform);
    let paint = PaintState { color, gradient, draw_style: DrawStyle::Fill, ..PaintState::default() };
    out.push(DrawCmd { path: imp, paint });
  }

  if let Some(stroke) = path.stroke() {
    let imp = build_imp_path(path.data(), &transform, FillType::NonZero);
    let (color, gradient) = resolve_paint(stroke.paint(), stroke.opacity(), &transform);
    let paint = PaintState {
      color,
      gradient,
      draw_style: DrawStyle::Stroke,
      stroke_width: stroke.width().get(),
      stroke_cap: match stroke.linecap() {
        usvg::LineCap::Butt => StrokeCap::Butt,
        usvg::LineCap::Round => StrokeCap::Round,
        usvg::LineCap::Square => StrokeCap::Square,
      },
      stroke_join: match stroke.linejoin() {
        usvg::LineJoin::Round => StrokeJoin::Round,
        usvg::LineJoin::Bevel => StrokeJoin::Bevel,
        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => StrokeJoin::Miter,
      },
      ..PaintState::default()
    };
    out.push(DrawCmd { path: imp, paint });
  }
}

fn build_imp_path(data: &usvg::tiny_skia_path::Path, t: &Transform, rule: FillType) -> ImpPath {
  let mut b = PathBuilder::default();
  let map = |p: usvg::tiny_skia_path::Point| -> Point {
    let mut p = p;
    t.map_point(&mut p);
    Point::new(p.x, p.y)
  };
  for seg in data.segments() {
    match seg {
      PathSegment::MoveTo(p) => {
        b.move_to(map(p));
      }
      PathSegment::LineTo(p) => {
        b.line_to(map(p));
      }
      PathSegment::QuadTo(c, p) => {
        b.quadratic_curve_to(map(c), map(p));
      }
      PathSegment::CubicTo(c1, c2, p) => {
        b.cubic_curve_to(map(c1), map(c2), map(p));
      }
      PathSegment::Close => {
        b.close();
      }
    }
  }
  b.take_path_new(rule)
}

// Resolves a usvg paint into a solid fallback color plus an optional gradient
// color source. The fallback (an averaged stop color for gradients, mid-gray for
// patterns) is what hit-testing and any uncovered region see; the gradient, when
// present, is what actually paints. `abs` is the path's absolute transform, which
// maps the gradient's coordinates into the same space as the baked geometry.
fn resolve_paint(paint: &usvg::Paint, opacity: usvg::Opacity, abs: &Transform) -> (Color, Option<Gradient>) {
  let alpha = opacity.get();
  match paint {
    usvg::Paint::Color(c) => (solid(c.red, c.green, c.blue, alpha), None),
    usvg::Paint::LinearGradient(grad) => {
      let (r, g, b) = average_stops(grad.stops());
      let gradient = Gradient::Linear {
        start: Point::new(grad.x1(), grad.y1()),
        end: Point::new(grad.x2(), grad.y2()),
        stops: convert_stops(grad.stops(), alpha),
        tile: spread_to_tile(grad.spread_method()),
        transform: transform_to_matrix(&abs.pre_concat(grad.transform())),
        units: GradientUnits::Absolute,
      };
      (solid(r, g, b, alpha), Some(gradient))
    }
    usvg::Paint::RadialGradient(grad) => {
      let (r, g, b) = average_stops(grad.stops());
      let gradient = Gradient::Radial {
        center: Point::new(grad.cx(), grad.cy()),
        radius: grad.r().get(),
        stops: convert_stops(grad.stops(), alpha),
        tile: spread_to_tile(grad.spread_method()),
        transform: transform_to_matrix(&abs.pre_concat(grad.transform())),
        units: GradientUnits::Absolute,
        circle: false,
      };
      (solid(r, g, b, alpha), Some(gradient))
    }
    usvg::Paint::Pattern(_) => (Color::new_srgba(0.5, 0.5, 0.5, alpha), None),
  }
}

fn solid(r: u8, g: u8, b: u8, a: f32) -> Color {
  Color::new_srgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

fn convert_stops(stops: &[usvg::Stop], fill_alpha: f32) -> Vec<GradientStop> {
  stops
    .iter()
    .map(|s| {
      let c = s.color();
      GradientStop {
        offset: s.offset().get(),
        color: solid(c.red, c.green, c.blue, s.opacity().get() * fill_alpha),
      }
    })
    .collect()
}

fn spread_to_tile(spread: usvg::SpreadMethod) -> TileMode {
  match spread {
    usvg::SpreadMethod::Pad => TileMode::Clamp,
    usvg::SpreadMethod::Reflect => TileMode::Mirror,
    usvg::SpreadMethod::Repeat => TileMode::Repeat,
  }
}

// tiny_skia affine (x' = sx*x + kx*y + tx, y' = ky*x + sy*y + ty) into the
// euclid-backed Impeller Matrix, whose new_2d takes column vectors (a,b)(c,d)(e,f).
fn transform_to_matrix(t: &Transform) -> Matrix {
  Matrix::new_2d(t.sx, t.ky, t.kx, t.sy, t.tx, t.ty)
}

// Mid-gray fallback color from the average of a gradient's stops, used for
// hit-testing and any region the color source does not cover.
fn average_stops(stops: &[usvg::Stop]) -> (u8, u8, u8) {
  if stops.is_empty() {
    return (128, 128, 128);
  }
  let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
  for s in stops {
    let c = s.color();
    r += c.red as u32;
    g += c.green as u32;
    b += c.blue as u32;
  }
  let n = stops.len() as u32;
  ((r / n) as u8, (g / n) as u8, (b / n) as u8)
}

fn color_to_hex(c: Color) -> String {
  let to_u8 = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
  format!("#{:02x}{:02x}{:02x}", to_u8(c.red), to_u8(c.green), to_u8(c.blue))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn has_color(built: &Built, r: f32, g: f32, b: f32) -> bool {
    built.cmds.iter().any(|cmd| {
      let c = &cmd.paint.color;
      (c.red - r).abs() < 0.02 && (c.green - g).abs() < 0.02 && (c.blue - b).abs() < 0.02
    })
  }

  #[test]
  fn parses_multicolor_shapes() {
    let mut svg = Svg::default();
    svg.set_src(
      r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
           <circle cx="50" cy="50" r="40" fill="#ff0000"/>
           <rect x="10" y="10" width="30" height="30" fill="#00ff00"/>
         </svg>"##
        .to_string(),
    );
    svg.ensure_built();
    let built = svg.built.borrow();
    let built = built.as_ref().expect("svg should parse");

    assert_eq!(built.intrinsic, (100.0, 100.0));
    // Two filled shapes -> two draws, with their own colors preserved.
    assert_eq!(built.cmds.len(), 2);
    assert!(has_color(built, 1.0, 0.0, 0.0), "red shape missing");
    assert!(has_color(built, 0.0, 1.0, 0.0), "green shape missing");
  }

  #[test]
  fn injects_current_color() {
    let mut svg = Svg::default();
    svg.set_color(Color::new_srgba(0.0, 0.0, 1.0, 1.0)); // blue
    svg.set_src(
      r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
           <path d="M5 12h14"/>
         </svg>"##
        .to_string(),
    );
    svg.ensure_built();
    let built = svg.built.borrow();
    let built = built.as_ref().expect("svg should parse");

    // One stroked path, colored via currentColor -> the injected blue.
    assert_eq!(built.cmds.len(), 1);
    assert_eq!(built.cmds[0].paint.draw_style, DrawStyle::Stroke);
    assert!(has_color(built, 0.0, 0.0, 1.0), "currentColor not applied");
  }

  #[test]
  fn builds_linear_gradient() {
    let mut svg = Svg::default();
    svg.set_src(
      r##"<svg viewBox="0 0 100 100">
           <defs><linearGradient id="g">
             <stop offset="0" stop-color="#000000"/>
             <stop offset="1" stop-color="#ffffff"/>
           </linearGradient></defs>
           <rect x="0" y="0" width="100" height="100" fill="url(#g)"/>
         </svg>"##
        .to_string(),
    );
    svg.ensure_built();
    let built = svg.built.borrow();
    let built = built.as_ref().expect("svg should parse");

    assert_eq!(built.cmds.len(), 1);
    // A real linear gradient with both stops, plus the averaged mid-gray fallback.
    match &built.cmds[0].paint.gradient {
      Some(Gradient::Linear { stops, .. }) => {
        assert_eq!(stops.len(), 2);
        assert!((stops[0].color.red - 0.0).abs() < 0.02, "first stop should be black");
        assert!((stops[1].color.red - 1.0).abs() < 0.02, "second stop should be white");
      }
      other => panic!("expected a linear gradient, got {other:?}"),
    }
    assert!(has_color(built, 0.5, 0.5, 0.5), "fallback should be averaged mid-gray");
  }
}