use super::{opt_f32, opt_radius};
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
    _ => return Ok(None),
  }))
}
