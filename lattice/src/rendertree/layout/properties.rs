use rquickjs::Value;
use taffy::prelude::*;

use super::util::{parse_dimension, parse_length_percentage, parse_length_percentage_auto};

pub fn set_property(style: &mut Style, property: &str, value: Value<'_>) -> Option<bool> {
  match property {
    "width"     => style.size.width      = parse_dimension(value),
    "height"    => style.size.height     = parse_dimension(value),
    "minWidth"  => style.min_size.width  = parse_dimension(value),
    "minHeight" => style.min_size.height = parse_dimension(value),
    "maxWidth"  => style.max_size.width  = parse_dimension(value),
    "maxHeight" => style.max_size.height = parse_dimension(value),

    "padding" => {
      let v = parse_length_percentage(value);
      style.padding = Rect { top: v, right: v, bottom: v, left: v };
    }
    "paddingTop"    => style.padding.top    = parse_length_percentage(value),
    "paddingRight"  => style.padding.right  = parse_length_percentage(value),
    "paddingBottom" => style.padding.bottom = parse_length_percentage(value),
    "paddingLeft"   => style.padding.left   = parse_length_percentage(value),

    "margin" => {
      let v = parse_length_percentage_auto(value);
      style.margin = Rect { top: v, right: v, bottom: v, left: v };
    }
    "marginTop"    => style.margin.top    = parse_length_percentage_auto(value),
    "marginRight"  => style.margin.right  = parse_length_percentage_auto(value),
    "marginBottom" => style.margin.bottom = parse_length_percentage_auto(value),
    "marginLeft"   => style.margin.left   = parse_length_percentage_auto(value),

    _ => return None,
  }
  Some(true)
}
