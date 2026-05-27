use super::PaintState;
use crate::rendertree::{
  BuildContext, Buildable, Element, ElementKind, Measurable, MeasureContext,
};
use alloy::impellers::{
  DisplayListBuilder, FontStyle, FontWeight, ParagraphBuilder, ParagraphStyle, Point, TextAlignment,
};
use rquickjs::Value;
use taffy::prelude::*;

#[derive(Clone, Debug)]
pub struct Text {
  pub computed_text: String,
  pub font_family: String,
  pub font_size: f32,
  pub font_style: FontStyle,
  pub font_weight: FontWeight,
  pub text_alignment: TextAlignment,
  pub max_lines: u32,
  pub line_height: f32,
  pub paint: PaintState,
}

impl Default for Text {
  fn default() -> Self {
    Self {
      computed_text: String::new(),
      font_family: "Noto Sans".to_string(),
      font_size: 20.0,
      font_style: FontStyle::Normal,
      font_weight: FontWeight::Medium,
      text_alignment: TextAlignment::Left,
      max_lines: 0,
      line_height: 0.0,
      paint: PaintState::default(),
    }
  }
}

impl Buildable for Text {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    let mut style = ParagraphStyle::default();
    let paint = self.paint.to_paint();
    style.set_foreground(&paint);
    style.set_font_family(&self.font_family);
    style.set_font_size(self.font_size);
    style.set_font_style(self.font_style);
    style.set_font_weight(self.font_weight);
    style.set_text_alignment(self.text_alignment);
    style.set_max_lines(self.max_lines);
    style.set_height(self.line_height);

    let Some(mut para_builder) = ParagraphBuilder::new(&ctx.platform.typography) else {
      return;
    };
    para_builder.push_style(&style);
    para_builder.add_text(&self.computed_text);

    let Some(paragraph) = para_builder.build(ctx.size.w) else {
      return;
    };
    builder.draw_paragraph(&paragraph, Point::new(0.0, 0.0));
  }
}

impl Measurable for Text {
  fn measure(&self, ctx: &MeasureContext) -> Size<f32> {
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size {
        width: w,
        height: h,
      };
    }

    let Some(mut para_builder) = ParagraphBuilder::new(&ctx.platform.typography) else {
      return Size::ZERO;
    };

    let mut style = ParagraphStyle::default();
    style.set_font_family(&self.font_family);
    style.set_font_size(self.font_size);
    style.set_font_style(self.font_style);
    style.set_font_weight(self.font_weight);
    style.set_max_lines(self.max_lines);
    style.set_height(self.line_height);

    para_builder.push_style(&style);
    para_builder.add_text(&self.computed_text);

    let Some(paragraph) = para_builder.build(f32::MAX) else {
      return Size::ZERO;
    };

    let max_intrinsic_width = paragraph.get_max_intrinsic_width();
    let min_intrinsic_width = paragraph.get_min_intrinsic_width();

    let width = ctx.known
      .width
      .unwrap_or_else(|| match ctx.available.width {
        AvailableSpace::Definite(w) => max_intrinsic_width.min(w),
        AvailableSpace::MaxContent => max_intrinsic_width,
        AvailableSpace::MinContent => min_intrinsic_width,
      });

    let Some(mut para_builder) = ParagraphBuilder::new(&ctx.platform.typography) else {
      return Size::ZERO;
    };
    para_builder.push_style(&style);
    para_builder.add_text(&self.computed_text);

    let Some(paragraph) = para_builder.build(width) else {
      return Size::ZERO;
    };

    let height = ctx.known
      .height
      .unwrap_or_else(|| paragraph.get_height());

    Size { width, height }
  }
}

impl Text {
  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "fontFamily" => {
        let s = value.get::<String>().expect("fontFamily must be a string");
        self.font_family = match s.as_str() {
          "mono" => "Noto Sans Mono".to_string(),
          "sans" => "Noto Sans".to_string(),
          other => other.to_string(),
        };
        Some(true)
      }
      "fontSize" => { self.font_size = value.get::<f64>().expect("fontSize must be a number") as f32; Some(true) }
      "lineHeight" => { self.line_height = value.get::<f64>().expect("lineHeight must be a number") as f32; Some(true) }
      "maxLines" => { self.max_lines = value.get::<f64>().expect("maxLines must be a number") as u32; Some(true) }
      "fontWeight" => {
        let w = value.get::<f64>().expect("fontWeight must be a number") as u32;
        self.font_weight = match w {
          100 => FontWeight::Thin,
          200 => FontWeight::ExtraLight,
          300 => FontWeight::Light,
          500 => FontWeight::Medium,
          600 => FontWeight::SemiBold,
          700 => FontWeight::Bold,
          800 => FontWeight::ExtraBold,
          900 => FontWeight::Black,
          _ => FontWeight::Regular,
        };
        Some(true)
      }
      _ => None,
    }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::Text(self),
      Style {
        display: Display::Block,
        ..Default::default()
      },
    )
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Text(self))
  }
}
