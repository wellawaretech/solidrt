use alloy::impellers::{FontStyle, FontWeight, TextAlignment};

use super::{opt, opt_f32, paint, str_of};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::text::layout::Wrap;
use alloy::rendertree::{OverflowWrap, Span, Text, TextAnchor, TextOverflow};

pub fn apply(text: &mut Text, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "x" => text.set_x(opt_f32(value, "x")?),
    "y" => text.set_y(opt_f32(value, "y")?),
    "w" => text.set_w(opt_f32(value, "w")?),
    "h" => text.set_h(opt_f32(value, "h")?),
    "anchor" => text.set_anchor(opt(value, |v| {
      Ok(match str_of(v, "anchor")? {
        "start" => TextAnchor::Start,
        "middle" => TextAnchor::Middle,
        "end" => TextAnchor::End,
        v => return Err(format!("Unknown anchor value \"{v}\"; expected start, middle or end")),
      })
    })?),
    // Role names ("sans", "serif", "mono") are registered font aliases; every
    // family name passes through to the typographer as-is.
    "fontFamily" => text.set_font_family(opt(value, |v| Ok(str_of(v, "fontFamily")?.to_string()))?),
    "fontSize" => text.set_font_size(opt_f32(value, "fontSize")?),
    "textAlign" => text.set_text_alignment(opt(value, |v| {
      Ok(match str_of(v, "textAlign")? {
        "left" => TextAlignment::Left,
        "right" => TextAlignment::Right,
        "center" => TextAlignment::Center,
        "justify" => TextAlignment::Justify,
        v => return Err(format!("Unknown textAlign value \"{v}\"; expected left, right, center or justify")),
      })
    })?),
    "lineHeight" => text.set_line_height(opt_f32(value, "lineHeight")?),
    "maxLines" => text.set_max_lines(opt_f32(value, "maxLines")?.map(|v| v as u32)),
    // "clip" | "ellipsis" | any other string, which is drawn as the ellipsis.
    "textOverflow" => text.set_text_overflow(opt(value, |v| {
      Ok(match str_of(v, "textOverflow")? {
        "clip" => TextOverflow::Clip,
        "ellipsis" => TextOverflow::Ellipsis("\u{2026}".to_string()),
        custom => TextOverflow::Ellipsis(custom.to_string()),
      })
    })?),
    "overflowWrap" => text.set_overflow_wrap(opt(value, |v| {
      Ok(match str_of(v, "overflowWrap")? {
        "normal" => OverflowWrap::Normal,
        "anywhere" => OverflowWrap::Anywhere,
        v => return Err(format!("Unknown overflowWrap value \"{v}\"; expected normal or anywhere")),
      })
    })?),
    "textIndent" => text.set_text_indent(opt_f32(value, "textIndent")?),
    "textWrap" => text.set_text_wrap(opt(value, |v| {
      Ok(match str_of(v, "textWrap")? {
        "wrap" => Wrap::Wrap,
        "balance" => Wrap::Balance,
        "pretty" => Wrap::Pretty,
        v => return Err(format!("Unknown textWrap value \"{v}\"; expected wrap, balance or pretty")),
      })
    })?),
    "fontStyle" => text.set_font_style(font_style_of(value)?),
    "fontWeight" => text.set_font_weight(font_weight_of(value)?),
    "textDecoration" => text.set_underline(underline_of(value)?),
    "textUnderlineOffset" => text.set_underline_offset(opt_f32(value, "textUnderlineOffset")?),
    "textDecorationThickness" => text.set_underline_thickness(opt_f32(value, "textDecorationThickness")?),
    _ => return Ok(None),
  }))
}

// A span takes the per-run subset of the text props as overrides; `color`
// (solid or gradient) writes into its paint override through the shared paint
// decoder, so a span accepts exactly what a text's color does.
pub fn apply_span(span: &mut Span, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    "text" => span.set_text(str_of(value, "text")?.to_string()),
    "fontFamily" => span.set_font_family(opt(value, |v| Ok(str_of(v, "fontFamily")?.to_string()))?),
    "fontSize" => span.set_font_size(opt_f32(value, "fontSize")?),
    "lineHeight" => span.set_line_height(opt_f32(value, "lineHeight")?),
    "fontStyle" => span.set_font_style(font_style_of(value)?),
    "fontWeight" => span.set_font_weight(font_weight_of(value)?),
    "textDecoration" => span.set_underline(underline_of(value)?),
    "textUnderlineOffset" => span.set_underline_offset(opt_f32(value, "textUnderlineOffset")?),
    "textDecorationThickness" => span.set_underline_thickness(opt_f32(value, "textDecorationThickness")?),
    // Null drops the override (inherit the paragraph's paint) rather than
    // resetting it to the default paint - see Span::clear_paint_override.
    "color" if value.is_null() => span.clear_paint_override(),
    "color" => match paint::apply(span.paint_override_mut(), name, value)? {
      Some(damage) => damage,
      None => return Ok(None),
    },
    _ => return Ok(None),
  }))
}

fn underline_of(value: &PropValue) -> Result<Option<bool>, String> {
  opt(value, |v| {
    Ok(match str_of(v, "textDecoration")? {
      "underline" => true,
      "none" => false,
      v => return Err(format!("Unknown textDecoration value \"{v}\"; expected none or underline")),
    })
  })
}

fn font_style_of(value: &PropValue) -> Result<Option<FontStyle>, String> {
  opt(value, |v| {
    Ok(match str_of(v, "fontStyle")? {
      "italic" => FontStyle::Italic,
      "normal" => FontStyle::Normal,
      v => return Err(format!("Unknown fontStyle value \"{v}\"; expected normal or italic")),
    })
  })
}

// Unlisted weights fall back to Regular by design (400 is the common case).
// Null resets: fontWeight is numeric on the JS surface, so it clears like
// the numbers (to the engine default on a text, to inherit on a span).
fn font_weight_of(value: &PropValue) -> Result<Option<FontWeight>, String> {
  let Some(v) = opt_f32(value, "fontWeight")? else { return Ok(None) };
  Ok(Some(match v as u32 {
    100 => FontWeight::Thin,
    200 => FontWeight::ExtraLight,
    300 => FontWeight::Light,
    500 => FontWeight::Medium,
    600 => FontWeight::SemiBold,
    700 => FontWeight::Bold,
    800 => FontWeight::ExtraBold,
    900 => FontWeight::Black,
    _ => FontWeight::Regular,
  }))
}
