use super::f32_of;
use crate::plugins::value::PropValue;
use crate::rendertree::Texture;

pub fn apply(tex: &mut Texture, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "src" => {
      // null/undefined clears, number sets the id.
      let id =
        if value.is_null() { None } else { Some(value.as_f64().expect("src must be a texture id (number)") as u64) };
      tex.set_src(id)
    }
    "srcX" => tex.set_src_x(f32_of(value, "srcX")),
    "srcY" => tex.set_src_y(f32_of(value, "srcY")),
    "srcW" => tex.set_src_w(f32_of(value, "srcW")),
    "srcH" => tex.set_src_h(f32_of(value, "srcH")),
    _ => return None,
  })
}
