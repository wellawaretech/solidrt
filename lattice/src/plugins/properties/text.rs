use alloy::impellers::FontWeight;

use super::{f32_of, str_of};
use crate::plugins::value::PropValue;
use crate::rendertree::{Span, Text};

pub fn apply(text: &mut Text, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "fontFamily" => text.set_font_family(match str_of(value, "fontFamily") {
      "mono" => "Noto Sans Mono".to_string(),
      "sans" => "Noto Sans".to_string(),
      other => other.to_string(),
    }),
    "fontSize" => text.set_font_size(f32_of(value, "fontSize")),
    "lineHeight" => text.set_line_height(f32_of(value, "lineHeight")),
    "maxLines" => text.set_max_lines(value.as_f64().expect("maxLines must be a number") as u32),
    "fontWeight" => text.set_font_weight(match value.as_f64().expect("fontWeight must be a number") as u32 {
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
    _ => return None,
  })
}

pub fn apply_span(span: &mut Span, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "text" => span.set_text(str_of(value, "text").to_string()),
    _ => return None,
  })
}
