use std::sync::mpsc::Sender;

use alloy::AlloyCommand;

use super::{decode_params, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::Window;
use alloy::WindowShader;

pub fn apply(win: &mut Window, name: &str, value: &PropValue, cmd_tx: &Sender<AlloyCommand>) -> Option<Damage> {
  Some(match name {
    "title" => win.set_title(str_of(value, "title").to_string(), cmd_tx),
    "fullscreen" => win.set_fullscreen(value.as_bool().expect("fullscreen must be a boolean"), cmd_tx),
    "shader" => win.set_shader(decode_shader(value)),
    _ => return None,
  })
}

// { program, params?, textures?, vertexCount?, previous? }; null clears.
// params and textures are name -> number maps (params like the texture
// element's, textures mapping sampler uniform names to texture ids);
// vertexCount defaults to 3, the covering triangle; previous defaults to
// false.
fn decode_shader(value: &PropValue) -> Option<WindowShader> {
  if value.is_null() {
    return None;
  }
  let program = value
    .get("program")
    .and_then(|v| v.as_f64())
    .expect("shader.program must be a program handle (number)") as u64;
  let params = value.get("params").map(decode_params).unwrap_or_default();
  let textures = value
    .get("textures")
    .and_then(|v| {
      v.as_map()
        .map(|entries| entries.iter().filter_map(|(k, t)| t.as_f64().map(|n| (k.clone(), n as u64))).collect())
    })
    .unwrap_or_default();
  let vertex_count = value.get("vertexCount").and_then(|v| v.as_f64()).map(|n| n as i32).unwrap_or(3);
  let previous = value.get("previous").and_then(|v| v.as_bool()).unwrap_or(false);
  Some(WindowShader { program, params, textures, vertex_count, previous })
}
