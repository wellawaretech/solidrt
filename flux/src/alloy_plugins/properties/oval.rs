use super::opt_f32;
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Oval;

pub fn apply(oval: &mut Oval, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x" => oval.set_x(opt_f32(value, "x")?),
    "y" => oval.set_y(opt_f32(value, "y")?),
    "w" => oval.set_w(opt_f32(value, "w")?),
    "h" => oval.set_h(opt_f32(value, "h")?),
    _ => return Ok(None),
  }))
}
