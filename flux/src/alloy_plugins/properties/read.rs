// The read half of the JSX property adapter: current values out of a node,
// under the same names and encodings `apply_jsx` accepts in. This is the one
// other place that knows the JSX vocabulary - keep every table here in sync
// with the decode tables in the sibling per-element files (the compiler
// enforces the enum coverage; the names it cannot).
//
// Consumed by the dev-server tree query (`get_render_tree` with props). The
// contract is off-default-only: an unset Option or a default enum emits
// nothing, so untouched nodes serialize to an empty list and payloads stay
// proportional to what the app actually set.

use alloy::impellers::{
  BlendMode, Color, DrawStyle, FillType, FontStyle, FontWeight, StrokeCap, StrokeJoin, TextAlignment,
};
use alloy::rendertree::{Element, ElementKind, Gradient, Line, OriginCoord, PaintState, TextureFit, View};

/// A read-back property value, kept engine- and serializer-free: the caller
/// (the dev connection) maps these onto its own JSON.
pub enum ReadValue {
  Num(f64),
  Int(i64),
  Bool(bool),
  Str(String),
  Nums(Vec<f64>),
}

/// Current property values of a node, JSX names and encodings, off-default
/// entries only. Order follows each kind's own field order.
pub fn read_jsx(element: &Element) -> Vec<(&'static str, ReadValue)> {
  let mut out: Vec<(&'static str, ReadValue)> = Vec::new();
  let num = |o: &mut Vec<(&'static str, ReadValue)>, name: &'static str, v: Option<f32>| {
    if let Some(v) = v {
      o.push((name, ReadValue::Num(v as f64)));
    }
  };
  match &element.kind {
    ElementKind::Window(win) => {
      if win.title != "SolidRT" {
        out.push(("title", ReadValue::Str(win.title.clone())));
      }
      if win.fullscreen {
        out.push(("fullscreen", ReadValue::Bool(true)));
      }
      if let Some(shader) = &win.shader {
        out.push(("shader", ReadValue::Int(shader.program as i64)));
      }
    }
    ElementKind::View(view) => read_view(view, &mut out),
    ElementKind::Rectangle(rect) => {
      num(&mut out, "x", rect.x);
      num(&mut out, "y", rect.y);
      num(&mut out, "w", rect.w);
      num(&mut out, "h", rect.h);
      if let Some(r) = rect.radius {
        out.push(("radius", ReadValue::Nums(r.iter().map(|v| *v as f64).collect())));
      }
    }
    ElementKind::Oval(oval) => {
      num(&mut out, "x", oval.x);
      num(&mut out, "y", oval.y);
      num(&mut out, "w", oval.w);
      num(&mut out, "h", oval.h);
    }
    ElementKind::Line(line) => {
      num(&mut out, "x1", line.x1);
      num(&mut out, "y1", line.y1);
      num(&mut out, "x2", line.x2);
      num(&mut out, "y2", line.y2);
      if let Some(points) = &line.points {
        out.push(("points", ReadValue::Nums(points.iter().map(|v| *v as f64).collect())));
      }
      if line.closed {
        out.push(("closed", ReadValue::Bool(true)));
      }
      num(&mut out, "onLength", line.on_length);
      num(&mut out, "offLength", line.off_length);
      num(&mut out, "dashOffset", line.dash_offset);
      num(&mut out, "pathLength", line.path_length);
    }
    ElementKind::Path(path) => {
      if !path.d.is_empty() {
        out.push(("d", ReadValue::Str(path.d.clone())));
      }
      num(&mut out, "x", path.x);
      num(&mut out, "y", path.y);
      if path.fill_rule != FillType::NonZero {
        out.push(("fillRule", ReadValue::Str("evenodd".into())));
      }
      num(&mut out, "onLength", path.on_length);
      num(&mut out, "offLength", path.off_length);
      num(&mut out, "dashOffset", path.dash_offset);
      num(&mut out, "pathLength", path.path_length);
    }
    ElementKind::Text(text) => {
      // `computed_text` itself already rides the snapshot as `text`.
      num(&mut out, "x", text.x);
      num(&mut out, "y", text.y);
      num(&mut out, "w", text.w);
      num(&mut out, "h", text.h);
      if text.font_family != "sans" {
        out.push(("fontFamily", ReadValue::Str(text.font_family.clone())));
      }
      if text.font_size != 20.0 {
        out.push(("fontSize", ReadValue::Num(text.font_size as f64)));
      }
      if text.font_style == FontStyle::Italic {
        out.push(("fontStyle", ReadValue::Str("italic".into())));
      }
      if text.font_weight != FontWeight::Medium {
        out.push(("fontWeight", ReadValue::Int(font_weight_number(text.font_weight))));
      }
      if text.text_alignment != TextAlignment::Left {
        out.push((
          "textAlign",
          ReadValue::Str(
            match text.text_alignment {
              TextAlignment::Left => "left",
              TextAlignment::Right => "right",
              TextAlignment::Center => "center",
              TextAlignment::Justify => "justify",
              // Not writable through the JSX adapter; read back honestly.
              TextAlignment::Start => "start",
              TextAlignment::End => "end",
            }
            .into(),
          ),
        ));
      }
      if text.max_lines != 0 {
        out.push(("maxLines", ReadValue::Int(text.max_lines as i64)));
      }
      if text.line_height != 0.0 {
        out.push(("lineHeight", ReadValue::Num(text.line_height as f64)));
      }
      if text.underline {
        out.push(("textDecoration", ReadValue::Str("underline".into())));
      }
      num(&mut out, "textUnderlineOffset", text.underline_offset);
      num(&mut out, "textDecorationThickness", text.underline_thickness);
    }
    ElementKind::Span(_) => {} // its text already rides the snapshot
    ElementKind::Texture(tex) => {
      if let Some(id) = tex.texture_id {
        out.push(("src", ReadValue::Int(id as i64)));
      }
      if tex.fit != TextureFit::Fill {
        out.push((
          "fit",
          ReadValue::Str(
            match tex.fit {
              TextureFit::Fill => "fill",
              TextureFit::Cover => "cover",
              TextureFit::Contain => "contain",
              TextureFit::None => "none",
              TextureFit::ScaleDown => "scale-down",
            }
            .into(),
          ),
        ));
      }
      num(&mut out, "srcX", tex.src_x);
      num(&mut out, "srcY", tex.src_y);
      num(&mut out, "srcW", tex.src_w);
      num(&mut out, "srcH", tex.src_h);
      num(&mut out, "x", tex.x);
      num(&mut out, "y", tex.y);
      num(&mut out, "w", tex.w);
      num(&mut out, "h", tex.h);
    }
  }
  // Overflow is layout style, applicable to any layouted kind. The write side
  // fans "overflow" out to both axes; read back the uniform name when they
  // agree and the per-axis names when they differ. Off-default = not Visible.
  if let Some(style) = element.style() {
    let name = |o: taffy::style::Overflow| match o {
      taffy::style::Overflow::Visible => "visible",
      taffy::style::Overflow::Hidden => "hidden",
      taffy::style::Overflow::Scroll => "scroll",
      taffy::style::Overflow::Clip => "clip",
    };
    let (x, y) = (style.overflow.x, style.overflow.y);
    if x == y {
      if x != taffy::style::Overflow::Visible {
        out.push(("overflow", ReadValue::Str(name(x).into())));
      }
    } else {
      if x != taffy::style::Overflow::Visible {
        out.push(("overflowX", ReadValue::Str(name(x).into())));
      }
      if y != taffy::style::Overflow::Visible {
        out.push(("overflowY", ReadValue::Str(name(y).into())));
      }
    }
  }
  if let Some(paint) = element.kind.paint() {
    let default_style = match &element.kind {
      ElementKind::Line(_) => Line::DEFAULT_DRAW_STYLE,
      _ => DrawStyle::Fill,
    };
    read_paint(paint, default_style, &mut out);
  }
  out
}

fn read_view(view: &View, out: &mut Vec<(&'static str, ReadValue)>) {
  let num = |o: &mut Vec<(&'static str, ReadValue)>, name: &'static str, v: Option<f32>| {
    if let Some(v) = v {
      o.push((name, ReadValue::Num(v as f64)));
    }
  };
  num(out, "rotate", view.rotate);
  // Written per axis (a uniform `scale` fans out on the way in), so read back
  // per axis; scaleX == scaleY is the uniform case.
  num(out, "scaleX", view.scale_x);
  num(out, "scaleY", view.scale_y);
  num(out, "rotateX", view.rotate_x);
  num(out, "rotateY", view.rotate_y);
  num(out, "perspective", view.perspective);
  if let Some(t) = view.translate {
    out.push(("x", ReadValue::Num(t.x as f64)));
    out.push(("y", ReadValue::Num(t.y as f64)));
  }
  if let Some(o) = view.origin_x {
    out.push(("originX", origin_value(o)));
  }
  if let Some(o) = view.origin_y {
    out.push(("originY", origin_value(o)));
  }
  num(out, "opacity", view.opacity);
  if let Some(s) = view.scroll {
    out.push(("scrollX", ReadValue::Num(s.x as f64)));
    out.push(("scrollY", ReadValue::Num(s.y as f64)));
  }
  if let Some(r) = view.clip_radius {
    out.push(("clipRadius", ReadValue::Nums(r.iter().map(|v| *v as f64).collect())));
  }
  if let Some(vb) = view.design_size {
    out.push(("designSize", ReadValue::Nums(vec![vb.width as f64, vb.height as f64])));
  }
  if let Some(shader) = &view.shader {
    out.push(("shader", ReadValue::Int(shader.program as i64)));
  }
}

// An origin axis reads back as the number it was set with; a fraction (pct()
// or a keyword on the way in) reads as a percent string.
fn origin_value(o: OriginCoord) -> ReadValue {
  match o {
    OriginCoord::Px(v) => ReadValue::Num(v as f64),
    OriginCoord::Fraction(f) => ReadValue::Str(format!("{}%", f * 100.0)),
  }
}

fn read_paint(paint: &PaintState, default_style: DrawStyle, out: &mut Vec<(&'static str, ReadValue)>) {
  if paint.gradient.is_some() {
    let kind = match paint.gradient {
      Some(Gradient::Linear { .. }) => "linear",
      Some(Gradient::Radial { .. }) => "radial",
      None => unreachable!(),
    };
    out.push(("color", ReadValue::Str(format!("gradient({kind})"))));
  } else {
    let default = PaintState::default().color;
    if !color_eq(paint.color, default) {
      out.push(("color", ReadValue::Str(color_hex(paint.color))));
    }
  }
  if paint.draw_style != default_style {
    out.push((
      "drawStyle",
      ReadValue::Str(
        match paint.draw_style {
          DrawStyle::Fill => "fill",
          DrawStyle::Stroke => "stroke",
          DrawStyle::StrokeAndFill => "stroke-and-fill",
        }
        .into(),
      ),
    ));
  }
  if paint.blend_mode != BlendMode::SourceOver {
    out.push(("blendMode", ReadValue::Str(blend_mode_name(paint.blend_mode).into())));
  }
  if paint.stroke_width != 0.0 {
    out.push(("strokeWidth", ReadValue::Num(paint.stroke_width as f64)));
  }
  if paint.stroke_cap != StrokeCap::Butt {
    out.push((
      "strokeCap",
      ReadValue::Str(
        match paint.stroke_cap {
          StrokeCap::Butt => "butt",
          StrokeCap::Round => "round",
          StrokeCap::Square => "square",
        }
        .into(),
      ),
    ));
  }
  if paint.stroke_join != StrokeJoin::Miter {
    out.push((
      "strokeJoin",
      ReadValue::Str(
        match paint.stroke_join {
          StrokeJoin::Miter => "miter",
          StrokeJoin::Round => "round",
          StrokeJoin::Bevel => "bevel",
        }
        .into(),
      ),
    ));
  }
  if paint.stroke_miter != 4.0 {
    out.push(("strokeMiter", ReadValue::Num(paint.stroke_miter as f64)));
  }
}

fn color_eq(a: Color, b: Color) -> bool {
  a.red == b.red && a.green == b.green && a.blue == b.blue && a.alpha == b.alpha
}

// #rrggbbaa, the read-back form of the packed-u32 write encoding.
fn color_hex(c: Color) -> String {
  let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
  format!("#{:02x}{:02x}{:02x}{:02x}", ch(c.red), ch(c.green), ch(c.blue), ch(c.alpha))
}

fn font_weight_number(w: FontWeight) -> i64 {
  match w {
    FontWeight::Thin => 100,
    FontWeight::ExtraLight => 200,
    FontWeight::Light => 300,
    FontWeight::Regular => 400,
    FontWeight::Medium => 500,
    FontWeight::SemiBold => 600,
    FontWeight::Bold => 700,
    FontWeight::ExtraBold => 800,
    FontWeight::Black => 900,
  }
}

fn blend_mode_name(mode: BlendMode) -> &'static str {
  match mode {
    BlendMode::Clear => "clear",
    BlendMode::Source => "source",
    BlendMode::Destination => "destination",
    BlendMode::SourceOver => "source-over",
    BlendMode::DestinationOver => "destination-over",
    BlendMode::SourceIn => "source-in",
    BlendMode::DestinationIn => "destination-in",
    BlendMode::SourceOut => "source-out",
    BlendMode::DestinationOut => "destination-out",
    BlendMode::SourceATop => "source-atop",
    BlendMode::DestinationATop => "destination-atop",
    BlendMode::Xor => "xor",
    BlendMode::Plus => "plus",
    BlendMode::Modulate => "modulate",
    BlendMode::Screen => "screen",
    BlendMode::Overlay => "overlay",
    BlendMode::Darken => "darken",
    BlendMode::Lighten => "lighten",
    BlendMode::ColorDodge => "color-dodge",
    BlendMode::ColorBurn => "color-burn",
    BlendMode::HardLight => "hard-light",
    BlendMode::SoftLight => "soft-light",
    BlendMode::Difference => "difference",
    BlendMode::Exclusion => "exclusion",
    BlendMode::Multiply => "multiply",
    BlendMode::Hue => "hue",
    BlendMode::Saturation => "saturation",
    BlendMode::Color => "color",
    BlendMode::Luminosity => "luminosity",
  }
}
