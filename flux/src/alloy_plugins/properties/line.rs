use super::opt_f32;
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Line;

pub fn apply(line: &mut Line, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x1" => line.set_x1(opt_f32(value, "x1")?),
    "y1" => line.set_y1(opt_f32(value, "y1")?),
    "x2" => line.set_x2(opt_f32(value, "x2")?),
    "y2" => line.set_y2(opt_f32(value, "y2")?),
    "onLength" => line.set_on_length(opt_f32(value, "onLength")?),
    "offLength" => line.set_off_length(opt_f32(value, "offLength")?),
    _ => return Ok(None),
  }))
}
