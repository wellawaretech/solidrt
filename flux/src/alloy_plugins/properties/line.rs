use super::{describe, f32_of, opt, opt_f32};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Line;

pub fn apply(line: &mut Line, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x1" => line.set_x1(opt_f32(value, "x1")?),
    "y1" => line.set_y1(opt_f32(value, "y1")?),
    "x2" => line.set_x2(opt_f32(value, "x2")?),
    "y2" => line.set_y2(opt_f32(value, "y2")?),
    "points" => line.set_points(opt(value, decode_points)?),
    // A null write resets to the kind's default, which for a line is stroke,
    // not PaintState's fill; the generic paint decoder takes every other
    // drawStyle value.
    "drawStyle" if value.is_null() => line.paint.set_draw_style(Some(Line::DEFAULT_DRAW_STYLE)),
    "closed" => line.set_closed(match value {
      PropValue::Null => false,
      _ => value.as_bool().ok_or_else(|| format!("closed must be a boolean, got {}", describe(value)))?,
    }),
    "onLength" => line.set_on_length(opt_f32(value, "onLength")?),
    "offLength" => line.set_off_length(opt_f32(value, "offLength")?),
    "dashOffset" => line.set_dash_offset(opt_f32(value, "dashOffset")?),
    _ => return Ok(None),
  }))
}

// A flat [x0, y0, x1, y1, ...] list of numbers; a Float32Array/Float64Array
// arrives as the same list (tree.rs to_prop_value). An odd count is rejected
// rather than silently dropping the trailing number.
fn decode_points(value: &PropValue) -> Result<Vec<f32>, String> {
  let items = value
    .as_list()
    .ok_or_else(|| format!("points must be an array of numbers [x0, y0, x1, y1, ...], got {}", describe(value)))?;
  if items.len() % 2 != 0 {
    return Err(format!("points must hold x, y pairs (an even count), got {} numbers", items.len()));
  }
  items.iter().enumerate().map(|(i, v)| f32_of(v, &format!("points[{i}]"))).collect()
}
