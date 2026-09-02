use super::{decode_shadow, opt_f32, opt_positive_f32, opt_radius};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Rectangle;

pub fn apply(rect: &mut Rectangle, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x" => rect.set_x(opt_f32(value, "x")?),
    "y" => rect.set_y(opt_f32(value, "y")?),
    "w" => rect.set_w(opt_f32(value, "w")?),
    "h" => rect.set_h(opt_f32(value, "h")?),
    "radius" => rect.set_radius(opt_radius(value, "radius")?),
    "onLength" => rect.set_on_length(opt_f32(value, "onLength")?),
    "offLength" => rect.set_off_length(opt_f32(value, "offLength")?),
    "dashOffset" => rect.set_dash_offset(opt_f32(value, "dashOffset")?),
    "pathLength" => rect.set_path_length(opt_positive_f32(value, "pathLength")?),
    "shadow" => rect.set_shadow(decode_shadow(value, true)?),
    _ => return Ok(None),
  }))
}
