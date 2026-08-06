use super::{as_pct_fraction, decode_params, decode_radius, decode_texture_bindings, describe, f32_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::OriginCoord;
use alloy::rendertree::View;
use alloy::NodeShader;

pub fn apply(view: &mut View, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "rotate" => view.set_rotate(f32_of(value, "rotate")?),
    "scale" => {
      // Uniform scale is a JS convenience; the rendertree is per-axis, so fan
      // it out to both. Last write wins if scale and scaleX/scaleY are mixed.
      let v = f32_of(value, "scale")?;
      view.set_scale_x(v);
      view.set_scale_y(v)
    }
    "scaleX" => view.set_scale_x(f32_of(value, "scaleX")?),
    "scaleY" => view.set_scale_y(f32_of(value, "scaleY")?),
    "rotateX" => view.set_rotate_x(f32_of(value, "rotateX")?),
    "rotateY" => view.set_rotate_y(f32_of(value, "rotateY")?),
    "perspective" => view.set_perspective(f32_of(value, "perspective")?),
    "x" => view.set_x(f32_of(value, "x")?),
    "y" => view.set_y(f32_of(value, "y")?),
    "originX" => view.set_origin_x(decode_origin_axis(value)?),
    "originY" => view.set_origin_y(decode_origin_axis(value)?),
    "opacity" => view.set_opacity(f32_of(value, "opacity")?),
    "scrollX" => view.set_scroll_x(f32_of(value, "scrollX")?),
    "scrollY" => view.set_scroll_y(f32_of(value, "scrollY")?),
    "clipRadius" => view.set_clip_radius(decode_radius(value)?),
    "viewBox" => {
      let list = value.as_list().ok_or_else(|| format!("viewBox must be a [w, h] list, got {}", describe(value)))?;
      if list.len() != 2 {
        return Err(format!("viewBox must have exactly [w, h], got {} entries", list.len()));
      }
      view.set_view_box(f32_of(&list[0], "viewBox w")?, f32_of(&list[1], "viewBox h")?)
    }
    "shader" => view.set_shader(decode_shader(value)?),
    _ => return Ok(None),
  }))
}

// { program, params?, textures?, outset?, previous? }; null clears. The
// window shader's shapes (params like the texture element's, textures
// mapping sampler uniform names to texture ids); outset is a non-negative
// logical-px margin; previous defaults to false. Applied only with
// repaintBoundary="snapshot"; the rendertree warns otherwise.
fn decode_shader(value: &PropValue) -> Result<Option<NodeShader>, String> {
  if value.is_null() {
    return Ok(None);
  }
  let program = value
    .get("program")
    .and_then(|v| v.as_f64())
    .ok_or_else(|| "shader.program must be a program handle (number)".to_string())? as u64;
  let params = value.get("params").map(decode_params).transpose()?.unwrap_or_default();
  let textures = value.get("textures").map(decode_texture_bindings).transpose()?.unwrap_or_default();
  let outset = match value.get("outset") {
    None => 0.0,
    Some(v) if v.is_null() => 0.0,
    Some(v) => f32_of(v, "shader.outset")?,
  };
  if !(outset >= 0.0 && outset.is_finite()) {
    return Err(format!("shader.outset must be a non-negative number, got {outset}"));
  }
  let previous = match value.get("previous") {
    None => false,
    Some(v) if v.is_null() => false,
    Some(v) => v.as_bool().ok_or_else(|| format!("shader.previous must be a boolean, got {}", describe(v)))?,
  };
  Ok(Some(NodeShader { program, params, textures, outset, previous }))
}

// One axis of the transform origin (originX / originY): a pixel number, a
// `pct(n)` fraction, or a position keyword (left/top = 0, center = 0.5,
// right/bottom = 1).
fn decode_origin_axis(value: &PropValue) -> Result<OriginCoord, String> {
  if let Some(n) = value.as_f64() {
    return Ok(OriginCoord::Px(n as f32));
  }
  if let Some(f) = as_pct_fraction(value)? {
    return Ok(OriginCoord::Fraction(f));
  }
  match value.as_str() {
    Some("left") | Some("top") => Ok(OriginCoord::Fraction(0.0)),
    Some("center") => Ok(OriginCoord::Fraction(0.5)),
    Some("right") | Some("bottom") => Ok(OriginCoord::Fraction(1.0)),
    Some(other) => {
      Err(format!("Unknown transformOrigin keyword \"{other}\"; expected left, top, center, right or bottom"))
    }
    None => Err(format!("transformOrigin value must be a number, pct(), or a keyword, got {}", describe(value))),
  }
}
