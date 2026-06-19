use alloy::impellers::FillType;

use super::{f32_of, str_of};
use crate::plugins::value::PropValue;
use alloy::rendertree::Path;

pub fn apply(path: &mut Path, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "d" => path.set_d(str_of(value, "d").to_string()),
    "x" => path.set_x(f32_of(value, "x")),
    "y" => path.set_y(f32_of(value, "y")),
    "fillRule" => path.set_fill_rule(match str_of(value, "fillRule") {
      "nonZero" => FillType::NonZero,
      "evenOdd" => FillType::Odd,
      v => panic!("unknown fillRule '{v}'"),
    }),
    _ => return None,
  })
}
