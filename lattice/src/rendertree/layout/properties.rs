use rquickjs::Value;
use taffy::prelude::*;

pub fn set_property(style: &mut Style, property: &str, value: Value<'_>) -> Option<bool> {
  match property {
    "width" => {
      let n = value.get::<f64>().expect("width must be a number") as f32;
      style.size.width = Dimension::length(n);
      Some(true)
    }
    "height" => {
      let n = value.get::<f64>().expect("height must be a number") as f32;
      style.size.height = Dimension::length(n);
      Some(true)
    }
    _ => None,
  }
}
