use rquickjs::Value;
use taffy::prelude::*;

use super::util::parse_dimension;

pub fn set_property(style: &mut Style, property: &str, value: Value<'_>) -> Option<bool> {
  match property {
    "width"     => style.size.width      = parse_dimension(value),
    "height"    => style.size.height     = parse_dimension(value),
    "minWidth"  => style.min_size.width  = parse_dimension(value),
    "minHeight" => style.min_size.height = parse_dimension(value),
    "maxWidth"  => style.max_size.width  = parse_dimension(value),
    "maxHeight" => style.max_size.height = parse_dimension(value),
    _ => return None,
  }
  Some(true)
}
