use super::{decode_params, describe, f32_of, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::{Texture, TextureFit};

pub fn apply(tex: &mut Texture, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "src" => {
      // null/undefined clears, number sets the id.
      let id = if value.is_null() {
        None
      } else {
        Some(value.as_f64().ok_or_else(|| format!("src must be a texture id (number), got {}", describe(value)))? as u64)
      };
      tex.set_src(id)
    }
    "fit" => {
      let fit = match str_of(value, "fit")? {
        "fill" => TextureFit::Fill,
        "cover" => TextureFit::Cover,
        "contain" => TextureFit::Contain,
        "none" => TextureFit::None,
        "scale-down" => TextureFit::ScaleDown,
        v => return Err(format!("Unknown fit value \"{v}\"; expected fill, cover, contain, none or scale-down")),
      };
      tex.set_fit(fit)
    }
    "srcX" => tex.set_src_x(f32_of(value, "srcX")?),
    "srcY" => tex.set_src_y(f32_of(value, "srcY")?),
    "srcW" => tex.set_src_w(f32_of(value, "srcW")?),
    "srcH" => tex.set_src_h(f32_of(value, "srcH")?),
    "x" => tex.set_x(f32_of(value, "x")?),
    "y" => tex.set_y(f32_of(value, "y")?),
    "w" => tex.set_w(f32_of(value, "w")?),
    "h" => tex.set_h(f32_of(value, "h")?),
    "params" => tex.set_params(decode_params(value)?),
    _ => return Ok(None),
  }))
}
