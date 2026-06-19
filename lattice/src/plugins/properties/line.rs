use super::f32_of;
use crate::plugins::value::PropValue;
use alloy::rendertree::Line;

pub fn apply(line: &mut Line, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "x1" => line.set_x1(f32_of(value, "x1")),
    "y1" => line.set_y1(f32_of(value, "y1")),
    "x2" => line.set_x2(f32_of(value, "x2")),
    "y2" => line.set_y2(f32_of(value, "y2")),
    "onLength" => line.set_on_length(f32_of(value, "onLength")),
    "offLength" => line.set_off_length(f32_of(value, "offLength")),
    _ => return None,
  })
}
