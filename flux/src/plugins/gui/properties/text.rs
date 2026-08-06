use alloy::impellers::{FontStyle, FontWeight, TextAlignment};

use super::{f32_of, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::{Span, Text};

pub fn apply(text: &mut Text, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x" => text.set_x(f32_of(value, "x")?),
    "y" => text.set_y(f32_of(value, "y")?),
    "w" => text.set_w(f32_of(value, "w")?),
    "h" => text.set_h(f32_of(value, "h")?),
    // Role names ("sans", "serif", "mono") are registered font aliases; every
    // family name passes through to the typographer as-is.
    "fontFamily" => text.set_font_family(str_of(value, "fontFamily")?.to_string()),
    "fontSize" => text.set_font_size(f32_of(value, "fontSize")?),
    "textAlign" => text.set_text_alignment(match str_of(value, "textAlign")? {
      "left" => TextAlignment::Left,
      "right" => TextAlignment::Right,
      "center" => TextAlignment::Center,
      "justify" => TextAlignment::Justify,
      v => return Err(format!("Unknown textAlign value \"{v}\"; expected left, right, center or justify")),
    }),
    "lineHeight" => text.set_line_height(f32_of(value, "lineHeight")?),
    "maxLines" => text.set_max_lines(f32_of(value, "maxLines")? as u32),
    "fontStyle" => text.set_font_style(match str_of(value, "fontStyle")? {
      "italic" => FontStyle::Italic,
      "normal" => FontStyle::Normal,
      v => return Err(format!("Unknown fontStyle value \"{v}\"; expected normal or italic")),
    }),
    // Unlisted weights fall back to Regular by design (400 is the common case).
    "fontWeight" => text.set_font_weight(match f32_of(value, "fontWeight")? as u32 {
      100 => FontWeight::Thin,
      200 => FontWeight::ExtraLight,
      300 => FontWeight::Light,
      500 => FontWeight::Medium,
      600 => FontWeight::SemiBold,
      700 => FontWeight::Bold,
      800 => FontWeight::ExtraBold,
      900 => FontWeight::Black,
      _ => FontWeight::Regular,
    }),
    _ => return Ok(None),
  }))
}

pub fn apply_span(span: &mut Span, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "text" => span.set_text(str_of(value, "text")?.to_string()),
    _ => return Ok(None),
  }))
}
