use super::{decode_params, decode_texture_bindings, describe, opt_f32, opt_radius};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::{Damage, Shader};

/// What a `<shader>` write asks the alloy context to check against the
/// element's paint shader before the element takes it: its params
/// (`Context::validate_paint_params`) or its sampler inputs
/// (`Context::validate_paint_inputs`). The element holds ids only, so the
/// uniform table is the context's.
pub enum PaintCheck<'a> {
  Params(&'a [(String, alloy::ParamValue)]),
  Inputs(&'a [alloy::TextureBinding]),
}

pub fn apply(
  shader: &mut Shader,
  name: &str,
  value: &PropValue,
  validate: &dyn Fn(u64, PaintCheck<'_>) -> Result<(), String>,
) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "src" => {
      // null/undefined clears, number sets the paint shader id.
      let id = if value.is_null() {
        None
      } else {
        Some(value.as_f64().ok_or_else(|| format!("src must be a paint shader id (number), got {}", describe(value)))?
          as u64)
      };
      shader.set_src(id)
    }
    // Element state, unlike a texture's params (which are its target's):
    // validated against the current src's uniforms, then held on the node,
    // a paint damage. Null or an empty object clears and needs no src.
    "params" => {
      let params = decode_params(value)?;
      if !params.is_empty() {
        let Some(id) = shader.shader_id else {
          return Err("params needs a paint shader to check against: set src before params".to_string());
        };
        validate(id, PaintCheck::Params(&params))?;
      }
      shader.set_params(params)
    }
    "textures" => {
      let textures = decode_texture_bindings(value)?;
      if !textures.is_empty() {
        let Some(id) = shader.shader_id else {
          return Err("textures needs a paint shader to check against: set src before textures".to_string());
        };
        validate(id, PaintCheck::Inputs(&textures))?;
      }
      shader.set_textures(textures)
    }
    "radius" => shader.set_radius(opt_radius(value, "radius")?),
    "x" => shader.set_x(opt_f32(value, "x")?),
    "y" => shader.set_y(opt_f32(value, "y")?),
    "w" => shader.set_w(opt_f32(value, "w")?),
    "h" => shader.set_h(opt_f32(value, "h")?),
    _ => return Ok(None),
  }))
}
