use alloy::impellers::{
  Color, DisplayListBuilder, Paint, ParagraphBuilder, ParagraphStyle, Point, TypographyContext,
};
use std::time::Instant;

pub fn fps() -> Box<dyn FnMut(&mut DisplayListBuilder, &alloy::FrameInfo)> {
  let mut typography: Option<TypographyContext> = None;
  let mut last_second = Instant::now();
  let mut frame_count: u32 = 0;
  let mut fps: u32 = 0;

  Box::new(move |b, info| {
    frame_count += 1;
    if info.frame_time.saturating_duration_since(last_second).as_secs_f32() >= 1.0 {
      fps = frame_count;
      frame_count = 0;
      last_second = info.frame_time;
    }

    let typo = typography.get_or_insert_with(TypographyContext::default);

    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 1.0, 1.0, 1.0));

    let mut style = ParagraphStyle::default();
    style.set_foreground(&paint);
    style.set_font_size(14.0);

    let Some(mut pb) = ParagraphBuilder::new(typo) else { return; };
    pb.push_style(&style);
    let text = format!("{} FPS", fps);
    pb.add_text(&text);

    let Some(paragraph) = pb.build(200.0) else { return; };
    let text_width = paragraph.get_max_intrinsic_width();
    let logical_w = (info.size.width as f32) / info.scale;
    let x = logical_w - text_width - 10.0;
    let y = 10.0;
    b.draw_paragraph(&paragraph, Point::new(x, y));
  })
}