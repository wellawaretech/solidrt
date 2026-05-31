use super::f32_of;
use crate::plugins::value::PropValue;
use crate::rendertree::View;

pub fn apply(view: &mut View, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "rotate" => view.set_rotate(f32_of(value, "rotate")),
    "scale" => view.set_scale(f32_of(value, "scale")),
    "x" => view.set_x(f32_of(value, "x")),
    "y" => view.set_y(f32_of(value, "y")),
    "cx" => view.set_cx(f32_of(value, "cx")),
    "cy" => view.set_cy(f32_of(value, "cy")),
    "scrollX" => view.set_scroll_x(f32_of(value, "scrollX")),
    "scrollY" => view.set_scroll_y(f32_of(value, "scrollY")),
    _ => return None,
  })
}