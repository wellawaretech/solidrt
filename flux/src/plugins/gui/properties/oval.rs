use super::f32_of;
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Oval;

pub fn apply(oval: &mut Oval, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "x" => oval.set_x(f32_of(value, "x")),
    "y" => oval.set_y(f32_of(value, "y")),
    "w" => oval.set_w(f32_of(value, "w")),
    "h" => oval.set_h(f32_of(value, "h")),
    _ => return None,
  })
}
