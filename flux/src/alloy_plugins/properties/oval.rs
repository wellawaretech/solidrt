use super::{opt_f32, opt_positive_f32};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Oval;

pub fn apply(oval: &mut Oval, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x" => oval.set_x(opt_f32(value, "x")?),
    "y" => oval.set_y(opt_f32(value, "y")?),
    "w" => oval.set_w(opt_f32(value, "w")?),
    "h" => oval.set_h(opt_f32(value, "h")?),
    "onLength" => oval.set_on_length(opt_f32(value, "onLength")?),
    "offLength" => oval.set_off_length(opt_f32(value, "offLength")?),
    "dashOffset" => oval.set_dash_offset(opt_f32(value, "dashOffset")?),
    "pathLength" => oval.set_path_length(opt_positive_f32(value, "pathLength")?),
    _ => return Ok(None),
  }))
}
