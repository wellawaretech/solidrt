use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Exception, Function, IntoJs, Object, Value};

use forge::svg::{SvgDocument, SvgDraw, SvgDrawStyle, SvgFillRule, SvgPaint, SvgSpread, SvgStrokeCap, SvgStrokeJoin};

use crate::plugins::marshal::OptArg;

// Marshalling for `flux:svg`: adapt JS args to the engine-free `forge::svg`
// parser and shape its plain draw data into the objects the render tree
// consumes directly - draw keys match the path element's props, solid colors
// come out as `#rrggbbaa` strings, gradients as the branded absolute-space
// gradient objects (see gui/properties/paint.rs for the decode).

pub struct SvgModule;

impl ModuleDef for SvgModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("parseSvg")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("parseSvg", Function::new(ctx.clone(), parse_svg)?)?;
    Ok(())
  }
}

fn parse_svg<'js>(ctx: Ctx<'js>, src: String, opts: OptArg<Object<'js>>) -> rquickjs::Result<Object<'js>> {
  let mut color: Option<u32> = None;
  if let Some(o) = opts.0 {
    let c: Value = o.get("color")?;
    if !c.is_undefined() && !c.is_null() {
      let Some(n) = c.as_number() else {
        return Err(Exception::throw_message(&ctx, "parseSvg color must be a packed 0xRRGGBBAA number"));
      };
      color = Some(n as u32);
    }
  }

  let doc = forge::svg::parse(&src, color).map_err(|msg| Exception::throw_message(&ctx, &msg))?;
  build_document(ctx, doc)
}

fn build_document<'js>(ctx: Ctx<'js>, doc: SvgDocument) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("width", doc.width)?;
  obj.set("height", doc.height)?;
  let draws = Array::new(ctx.clone())?;
  for (i, draw) in doc.draws.into_iter().enumerate() {
    draws.set(i, build_draw(ctx.clone(), draw)?)?;
  }
  obj.set("draws", draws)?;
  Ok(obj)
}

fn build_draw<'js>(ctx: Ctx<'js>, draw: SvgDraw) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("d", draw.d)?;
  obj.set(
    "drawStyle",
    match draw.style {
      SvgDrawStyle::Fill => "fill",
      SvgDrawStyle::Stroke => "stroke",
    },
  )?;
  obj.set("color", build_color(ctx.clone(), draw.paint)?)?;
  // Optional keys are omitted, not set to undefined, so spreading a draw onto
  // an element never writes an undefined prop.
  if let Some(rule) = draw.fill_rule {
    obj.set(
      "fillRule",
      match rule {
        SvgFillRule::NonZero => "nonzero",
        SvgFillRule::EvenOdd => "evenodd",
      },
    )?;
  }
  if let Some(width) = draw.stroke_width {
    obj.set("strokeWidth", width)?;
  }
  if let Some(cap) = draw.stroke_cap {
    obj.set(
      "strokeCap",
      match cap {
        SvgStrokeCap::Butt => "butt",
        SvgStrokeCap::Round => "round",
        SvgStrokeCap::Square => "square",
      },
    )?;
  }
  if let Some(join) = draw.stroke_join {
    obj.set(
      "strokeJoin",
      match join {
        SvgStrokeJoin::Miter => "miter",
        SvgStrokeJoin::Round => "round",
        SvgStrokeJoin::Bevel => "bevel",
      },
    )?;
  }
  Ok(obj)
}

fn build_color<'js>(ctx: Ctx<'js>, paint: SvgPaint) -> rquickjs::Result<Value<'js>> {
  match paint {
    SvgPaint::Solid(c) => format!("#{c:08x}").into_js(&ctx),
    SvgPaint::Linear { x0, y0, x1, y1, stops, spread, transform } => {
      let obj = gradient_object(ctx.clone(), "linear", stops, spread, transform)?;
      obj.set("x0", x0)?;
      obj.set("y0", y0)?;
      obj.set("x1", x1)?;
      obj.set("y1", y1)?;
      obj.into_js(&ctx)
    }
    SvgPaint::Radial { cx, cy, r, stops, spread, transform } => {
      let obj = gradient_object(ctx.clone(), "radial", stops, spread, transform)?;
      obj.set("cx", cx)?;
      obj.set("cy", cy)?;
      obj.set("r", r)?;
      obj.into_js(&ctx)
    }
  }
}

// The shared part of the branded gradient objects. `spread` and `transform`
// are only set when they differ from the decode defaults (pad / identity).
fn gradient_object<'js>(
  ctx: Ctx<'js>,
  kind: &str,
  stops: Vec<forge::svg::SvgStop>,
  spread: SvgSpread,
  transform: [f32; 6],
) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("__gradient", kind)?;
  obj.set("units", "absolute")?;
  let arr = Array::new(ctx.clone())?;
  for (i, stop) in stops.into_iter().enumerate() {
    let s = Object::new(ctx.clone())?;
    s.set("offset", stop.offset)?;
    // As f64, never u32: rquickjs converts a u32 whose i32 cast roundtrips
    // (any color with a high red byte) into a NEGATIVE JS int, which the
    // property decode's `as u32` then saturates to transparent black.
    s.set("color", stop.color as f64)?;
    arr.set(i, s)?;
  }
  obj.set("stops", arr)?;
  match spread {
    SvgSpread::Pad => {}
    SvgSpread::Reflect => obj.set("spread", "reflect")?,
    SvgSpread::Repeat => obj.set("spread", "repeat")?,
  }
  if transform != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] {
    obj.set("transform", transform.to_vec())?;
  }
  Ok(obj)
}
