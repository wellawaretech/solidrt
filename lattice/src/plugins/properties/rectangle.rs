use super::f32_of;
use crate::plugins::value::PropValue;
use crate::rendertree::Rectangle;

pub fn apply(rect: &mut Rectangle, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "x" => rect.set_x(f32_of(value, "x")),
    "y" => rect.set_y(f32_of(value, "y")),
    "w" => rect.set_w(f32_of(value, "w")),
    "h" => rect.set_h(f32_of(value, "h")),
    "radius" => rect.set_radius(decode_radius(value)),
    _ => return None,
  })
}

// A single number applies to all four corners; an array is
// [top-left, top-right, bottom-right, bottom-left].
fn decode_radius(value: &PropValue) -> [f32; 4] {
  if let Some(arr) = value.as_list() {
    if arr.len() != 4 {
      panic!("radius array must have 4 elements [top-left, top-right, bottom-right, bottom-left]");
    }
    [
      arr[0].as_f64().expect("radius[0] must be a number") as f32,
      arr[1].as_f64().expect("radius[1] must be a number") as f32,
      arr[2].as_f64().expect("radius[2] must be a number") as f32,
      arr[3].as_f64().expect("radius[3] must be a number") as f32,
    ]
  } else {
    let v = value.as_f64().expect("radius must be a number or an array of 4 numbers") as f32;
    [v, v, v, v]
  }
}