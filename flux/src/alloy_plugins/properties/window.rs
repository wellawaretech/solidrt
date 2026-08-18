use std::sync::mpsc::Sender;

use alloy::AlloyCommand;

use super::{decode_params, decode_texture_bindings, describe, str_of};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Window;
use alloy::WindowShader;

pub fn apply(
  win: &mut Window,
  name: &str,
  value: &PropValue,
  cmd_tx: &Sender<AlloyCommand>,
) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "title" => win.set_title(str_of(value, "title")?.to_string(), cmd_tx),
    "fullscreen" => win.set_fullscreen(
      value.as_bool().ok_or_else(|| format!("fullscreen must be a boolean, got {}", describe(value)))?,
      cmd_tx,
    ),
    "shader" => win.set_shader(decode_shader(value)?),
    _ => return Ok(None),
  }))
}

// { program, params?, textures?, vertexCount?, previous? }; null clears.
// params and textures are name -> number maps (params like the texture
// element's, textures mapping sampler uniform names to texture ids);
// vertexCount defaults to 3, the covering triangle; previous defaults to
// false.
fn decode_shader(value: &PropValue) -> Result<Option<WindowShader>, String> {
  if value.is_null() {
    return Ok(None);
  }
  let program = value
    .get("program")
    .and_then(|v| v.as_f64())
    .ok_or_else(|| "shader.program must be a program handle (number)".to_string())? as u64;
  let params = value.get("params").map(decode_params).transpose()?.unwrap_or_default();
  let textures = value.get("textures").map(decode_texture_bindings).transpose()?.unwrap_or_default();
  let vertex_count = match value.get("vertexCount") {
    None => 3,
    Some(v) if v.is_null() => 3,
    Some(v) => v.as_f64().ok_or_else(|| format!("shader.vertexCount must be a number, got {}", describe(v)))? as i32,
  };
  let previous = match value.get("previous") {
    None => false,
    Some(v) if v.is_null() => false,
    Some(v) => v.as_bool().ok_or_else(|| format!("shader.previous must be a boolean, got {}", describe(v)))?,
  };
  Ok(Some(WindowShader { program, params, textures, vertex_count, previous }))
}
