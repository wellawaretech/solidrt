use alloy::impellers::{FontStyle, FontWeight, TextAlignment};

use super::{f32_of, paint, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::{OverflowWrap, Span, Text, TextLayoutMode, TextOverflow};

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
    // "clip" | "ellipsis" | any other string, which is drawn as the ellipsis.
    "textOverflow" => text.set_text_overflow(match str_of(value, "textOverflow")? {
      "clip" => TextOverflow::Clip,
      "ellipsis" => TextOverflow::Ellipsis("\u{2026}".to_string()),
      custom => TextOverflow::Ellipsis(custom.to_string()),
    }),
    "overflowWrap" => text.set_overflow_wrap(match str_of(value, "overflowWrap")? {
      "normal" => OverflowWrap::Normal,
      "anywhere" => OverflowWrap::Anywhere,
      v => return Err(format!("Unknown overflowWrap value \"{v}\"; expected normal or anywhere")),
    }),
    "textIndent" => text.set_text_indent(f32_of(value, "textIndent")?),
    "textLayout" => text.set_layout_mode(match str_of(value, "textLayout")? {
      "paragraph" => TextLayoutMode::Paragraph,
      "owned" => TextLayoutMode::Owned,
      v => return Err(format!("Unknown textLayout value \"{v}\"; expected paragraph or owned")),
    }),
    "fontStyle" => text.set_font_style(font_style_of(value)?),
    "fontWeight" => text.set_font_weight(font_weight_of(value)?),
    _ => return Ok(None),
  }))
}

// A span takes the per-run subset of the text props as overrides; `color`
// (solid or gradient) writes into its paint override through the shared paint
// decoder, so a span accepts exactly what a text's color does.
pub fn apply_span(span: &mut Span, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "text" => span.set_text(str_of(value, "text")?.to_string()),
    "fontFamily" => span.set_font_family(str_of(value, "fontFamily")?.to_string()),
    "fontSize" => span.set_font_size(f32_of(value, "fontSize")?),
    "lineHeight" => span.set_line_height(f32_of(value, "lineHeight")?),
    "fontStyle" => span.set_font_style(font_style_of(value)?),
    "fontWeight" => span.set_font_weight(font_weight_of(value)?),
    "color" => match paint::apply(span.paint_override_mut(), name, value)? {
      Some(damage) => damage,
      None => return Ok(None),
    },
    _ => return Ok(None),
  }))
}

fn font_style_of(value: &PropValue) -> Result<FontStyle, String> {
  Ok(match str_of(value, "fontStyle")? {
    "italic" => FontStyle::Italic,
    "normal" => FontStyle::Normal,
    v => return Err(format!("Unknown fontStyle value \"{v}\"; expected normal or italic")),
  })
}

// Unlisted weights fall back to Regular by design (400 is the common case).
fn font_weight_of(value: &PropValue) -> Result<FontWeight, String> {
  Ok(match f32_of(value, "fontWeight")? as u32 {
    100 => FontWeight::Thin,
    200 => FontWeight::ExtraLight,
    300 => FontWeight::Light,
    500 => FontWeight::Medium,
    600 => FontWeight::SemiBold,
    700 => FontWeight::Bold,
    800 => FontWeight::ExtraBold,
    900 => FontWeight::Black,
    _ => FontWeight::Regular,
  })
}
