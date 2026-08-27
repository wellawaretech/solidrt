use super::{as_pct_fraction, decode_params, decode_texture_bindings, describe, f32_of, opt_f32, opt_radius};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::OriginCoord;
use alloy::rendertree::View;
use alloy::NodeShader;

// Null on any of these resets the prop to its unset default (see the Option
// setters in alloy); a non-numeric non-null value is still an error.
pub fn apply(view: &mut View, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "rotate" => view.set_rotate(opt_f32(value, "rotate")?),
    "scale" => {
      // Uniform scale is a JS convenience; the rendertree is per-axis, so fan
      // it out to both. Last write wins if scale and scaleX/scaleY are mixed.
      let v = opt_f32(value, "scale")?;
      view.set_scale_x(v);
      view.set_scale_y(v)
    }
    "scaleX" => view.set_scale_x(opt_f32(value, "scaleX")?),
    "scaleY" => view.set_scale_y(opt_f32(value, "scaleY")?),
    "rotateX" => view.set_rotate_x(opt_f32(value, "rotateX")?),
    "rotateY" => view.set_rotate_y(opt_f32(value, "rotateY")?),
    "perspective" => view.set_perspective(opt_f32(value, "perspective")?),
    "x" => view.set_x(opt_f32(value, "x")?),
    "y" => view.set_y(opt_f32(value, "y")?),
    "originX" => view.set_origin_x(decode_origin_axis(value)?),
    "originY" => view.set_origin_y(decode_origin_axis(value)?),
    "opacity" => view.set_opacity(opt_f32(value, "opacity")?),
    "scrollX" => view.set_scroll_x(opt_f32(value, "scrollX")?),
    "scrollY" => view.set_scroll_y(opt_f32(value, "scrollY")?),
    "clipRadius" => view.set_clip_radius(opt_radius(value, "clipRadius")?),
    "designSize" => {
      if value.is_null() {
        return Ok(Some(view.set_design_size(None)));
      }
      let list = value.as_list().ok_or_else(|| format!("designSize must be a [w, h] list, got {}", describe(value)))?;
      if list.len() != 2 {
        return Err(format!("designSize must have exactly [w, h], got {} entries", list.len()));
      }
      let (w, h) = (f32_of(&list[0], "designSize w")?, f32_of(&list[1], "designSize h")?);
      // A design space needs a positive, finite extent on both axes: anything
      // else has no fit scale and would hand the children a degenerate frame
      // (View::design_space).
      if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return Err(format!("designSize must be a positive, finite [w, h], got [{w}, {h}]"));
      }
      view.set_design_size(Some((w, h)))
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
// right/bottom = 1). Null resets to the unset default (box center).
fn decode_origin_axis(value: &PropValue) -> Result<Option<OriginCoord>, String> {
  if value.is_null() {
    return Ok(None);
  }
  if let Some(n) = value.as_f64() {
    return Ok(Some(OriginCoord::Px(n as f32)));
  }
  if let Some(f) = as_pct_fraction(value)? {
    return Ok(Some(OriginCoord::Fraction(f)));
  }
  match value.as_str() {
    Some("left") | Some("top") => Ok(Some(OriginCoord::Fraction(0.0))),
    Some("center") => Ok(Some(OriginCoord::Fraction(0.5))),
    Some("right") | Some("bottom") => Ok(Some(OriginCoord::Fraction(1.0))),
    Some(other) => {
      Err(format!("Unknown transformOrigin keyword \"{other}\"; expected left, top, center, right or bottom"))
    }
    None => Err(format!("transformOrigin value must be a number, pct(), or a keyword, got {}", describe(value))),
  }
}
