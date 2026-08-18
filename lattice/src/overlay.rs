use alloy::impellers::{
  Color, DisplayListBuilder, Paint, ParagraphBuilder, ParagraphStyle, Point, Rect, Size, TextAlignment,
  TypographyContext,
};

use crate::stats::StatsSnapshot;

const MIB: f32 = 1024.0 * 1024.0;
const PARA_WIDTH: f32 = 200.0;

/// Build the overlay declaration the raster thread composites over every
/// finished frame (after any window shader pass): a display list with the
/// HUD drawn at the origin, plus the window-space rectangle it belongs in
/// (physical pixels - `scale` is the display scale the app's own lists are
/// built with, so the HUD lays out in the same logical coordinates
/// `safe_area` is in). None when a paragraph cannot be built.
pub fn build(s: &StatsSnapshot, typography: &TypographyContext, safe_area: Rect, scale: f32) -> Option<alloy::StatsOverlay> {
  let mut b = DisplayListBuilder::new(None);
  b.scale(scale, scale);
  let paint_stats = s.paint;
  let mut paint = Paint::default();
  paint.set_color(Color::new_srgba(1.0, 1.0, 1.0, 1.0));

  let mut style = ParagraphStyle::default();
  style.set_foreground(&paint);
  style.set_font_family("mono");
  style.set_font_size(14.0);
  style.set_font_weight(alloy::impellers::FontWeight::Bold);
  style.set_text_alignment(TextAlignment::Right);

  let Some(mut pb) = ParagraphBuilder::new(typography) else {
    return None;
  };
  pb.push_style(&style);

  let mut text = format!("{:.0}% CPU {:.0} MEM {} FPS", s.cpu_pct, s.mem_bytes as f32 / MIB, s.fps);
  // Each timing is shown as a share of the measured frame period (js_ms and
  // frame_ms are smoothed the same way on the JS thread, so a share stays
  // within 100%). Shares sum to ~100% when CPU-bound; less means idle or
  // GPU-bound headroom. A share is relative to the current frame, so one phase
  // shrinks when another grows. JS = onFrame + flush; LAY/PNT/PST/HOV = native
  // draw phases. SET is a raw count (setProperty writes/frame), not a share.
  let frame_ms = s.frame_ms;
  let pct = |ms: f32| if frame_ms > 0.0 { ms / frame_ms * 100.0 } else { 0.0 };
  text.push_str(&format!("\nJS {:.0}% SET {:.0}", pct(s.js_ms), s.set_count));
  // Native draw phases as frame shares: LAY layout, PNT paint, PST postLayout,
  // HOV hover.
  text.push_str(&format!(
    "\nLAY {:.0}% PNT {:.0}%\nPST {:.0}% HOV {:.0}%",
    pct(s.layout_ms),
    pct(s.paint_ms),
    pct(s.post_ms),
    pct(s.hover_ms),
  ));
  // Demand-gate savings/sec: frames served from the cached display list
  // (reuse) and frames skipped entirely (skip). Hidden when the gate saved
  // nothing this second - every frame a full rebuild, which FPS already shows.
  if s.reused + s.skipped > 0 {
    text.push_str(&format!("\n{} reuse {} skip", s.reused, s.skipped));
  }
  // Repaint boundaries this frame: reused+recorded. Hidden when the app
  // declares none.
  if paint_stats.boundaries_reused + paint_stats.boundaries_recorded > 0 {
    text.push_str(&format!("\n{}+{} BND", paint_stats.boundaries_reused, paint_stats.boundaries_recorded));
  }
  // Snapshot boundaries this frame: reused+rerendered+rasterized (drawn from
  // the retained texture, re-rendered into retained storage, freshly
  // allocated).
  if paint_stats.snapshots_reused + paint_stats.snapshots_rerendered + paint_stats.snapshots_rasterized > 0 {
    text.push_str(&format!(
      "\n{}+{}+{} SNP",
      paint_stats.snapshots_reused, paint_stats.snapshots_rerendered, paint_stats.snapshots_rasterized
    ));
  }
  // Textures currently held in the registry (GL/Impeller texture pairs in use).
  if s.textures > 0 {
    text.push_str(&format!("\n{} TEX", s.textures));
  }

  pb.add_text(&text);

  let Some(paragraph) = pb.build(PARA_WIDTH) else {
    return None;
  };

  // Darkening backdrop so the white text stays legible over light content,
  // drawn at the origin: placement travels as the declaration's rectangle.
  let pad = 10.0;
  let text_w = paragraph.get_longest_line_width();
  let text_h = paragraph.get_height();
  let w = text_w + pad * 2.0;
  let h = text_h + pad * 2.0;
  let mut bg_paint = Paint::default();
  bg_paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, 0.7));
  b.draw_rect(&Rect::new(Point::new(0.0, 0.0), Size::new(w, h)), &bg_paint);
  // The paragraph is right-aligned in PARA_WIDTH: place it so its right
  // edge sits one pad inside the backdrop's right edge.
  b.draw_paragraph(&paragraph, Point::new(pad + text_w - PARA_WIDTH, pad));

  // Same anchor the in-tree overlay drew at: the backdrop's right edge 10
  // logical px inside the safe area's top-right corner, its top flush with
  // the safe area's top.
  let win_x = safe_area.origin.x + safe_area.size.width - 10.0 - text_w - pad;
  let win_y = safe_area.origin.y + 10.0 - pad;
  let dl = b.build()?;
  Some(alloy::StatsOverlay {
    dl,
    x: (win_x * scale).round() as i32,
    y: (win_y * scale).round() as i32,
    width: (w * scale).ceil() as u32,
    height: (h * scale).ceil() as u32,
  })
}
