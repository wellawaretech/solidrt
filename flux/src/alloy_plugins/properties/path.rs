use alloy::impellers::FillType;

use super::{decode_shadow, opt, opt_f32, opt_positive_f32, str_of};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Path;

pub fn apply(path: &mut Path, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "d" => path.set_d(str_of(value, "d")?.to_string()),
    "x" => path.set_x(opt_f32(value, "x")?),
    "y" => path.set_y(opt_f32(value, "y")?),
    "onLength" => path.set_on_length(opt_f32(value, "onLength")?),
    "offLength" => path.set_off_length(opt_f32(value, "offLength")?),
    "dashOffset" => path.set_dash_offset(opt_f32(value, "dashOffset")?),
    "pathLength" => path.set_path_length(opt_positive_f32(value, "pathLength")?),
    "fillRule" => path.set_fill_rule(opt(value, |v| {
      Ok(match str_of(v, "fillRule")? {
        "nonzero" => FillType::NonZero,
        "evenodd" => FillType::Odd,
        v => return Err(format!("Unknown fillRule \"{v}\"; expected nonzero or evenodd")),
      })
    })?),
    "shadow" => path.set_shadow(decode_shadow(value, false)?),
    _ => return Ok(None),
  }))
}
