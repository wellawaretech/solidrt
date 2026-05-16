use rquickjs::Value;
use taffy::prelude::*;

use super::util::parse_dimension;

pub fn set_property(style: &mut Style, property: &str, value: Value<'_>) -> Option<bool> {
  match property {
    "width" => {
      style.size.width = parse_dimension(value);
      Some(true)
    }
    "height" => {
      style.size.height = parse_dimension(value);
      Some(true)
    }
    "minWidth" => {
      style.min_size.width = parse_dimension(value);
      Some(true)
    }
    "minHeight" => {
      style.min_size.height = parse_dimension(value);
      Some(true)
    }
    "maxWidth" => {
      style.max_size.width = parse_dimension(value);
      Some(true)
    }
    "maxHeight" => {
      style.max_size.height = parse_dimension(value);
      Some(true)
    }
    _ => None,
  }
}
