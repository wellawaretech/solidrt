use super::{decode_color, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Svg;

pub fn apply(svg: &mut Svg, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "src" => svg.set_src(str_of(value, "src").to_string()),
    // Drives currentColor in the document; explicit fills/strokes still win.
    "color" => svg.set_color(decode_color(value)),
    _ => return None,
  })
}
