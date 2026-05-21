use alloy::impellers::{
  Color, DisplayListBuilder, Paint, ParagraphBuilder, ParagraphStyle, Point, Rect,
  TypographyContext,
};

pub fn fps(b: &mut DisplayListBuilder, typography: &TypographyContext, safe_area: Rect, fps: u32) {
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 1.0, 1.0, 1.0));

  let mut style = ParagraphStyle::default();
  style.set_foreground(&paint);
  style.set_font_size(14.0);

  let Some(mut pb) = ParagraphBuilder::new(typography) else { return; };
  pb.push_style(&style);
  let text = format!("{} FPS", fps);
  pb.add_text(&text);

  let Some(paragraph) = pb.build(200.0) else { return; };
  let text_width = paragraph.get_max_intrinsic_width();
  let x = safe_area.origin.x + safe_area.size.width - text_width - 10.0;
  let y = safe_area.origin.y + 10.0;
  b.draw_paragraph(&paragraph, Point::new(x, y));
}