use super::{decode_radius, f32_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Rectangle;

pub fn apply(rect: &mut Rectangle, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x" => rect.set_x(f32_of(value, "x")?),
    "y" => rect.set_y(f32_of(value, "y")?),
    "w" => rect.set_w(f32_of(value, "w")?),
    "h" => rect.set_h(f32_of(value, "h")?),
    "radius" => rect.set_radius(decode_radius(value)?),
    _ => return Ok(None),
  }))
}
