use alloy::impellers::{
  Color, DisplayListBuilder, FontStyle, FontWeight, Paint, Point, Rect, Size, TextAlignment, TextureSampling,
};
use alloy::rendertree::text::glyphs::{weight_value, WarmRequest};
use alloy::rendertree::text::TextImage;
use alloy::rendertree::{PlatformContext, Text};

use crate::stats::StatsSnapshot;

const MIB: f32 = 1024.0 * 1024.0;
// Wrap width of the text, logical px: wide enough that the longest first
// line (badge + headline figures + FPS, e.g. "MUTED 100% CPU 1024 MEM 120 FPS"
// in 14 px bold mono) never wraps. The backdrop is sized from the placed
// ink, not this.
const PARA_WIDTH: f32 = 300.0;
// The HUD's type: the mono role, bold, at this size.
const FONT_SIZE: f32 = 14.0;
// The backdrop's inset around the text, logical px.
const PAD: f32 = 10.0;
// How far the text sits inside the safe area's top-right corner.
const INSET: f32 = 10.0;
// The darkening backdrop's opacity: enough to keep white text legible over
// light content.
const BACKDROP_ALPHA: f32 = 0.7;

// The overlay's text node before its text: the one style the HUD is built
// in and warmed for.
fn hud_text() -> Text {
  let mut node = Text::default();
  node.font_family = "mono".to_string();
  node.font_size = FONT_SIZE;
  node.font_weight = FontWeight::Bold;
  node.text_alignment = Some(TextAlignment::Right);
  node.w = Some(PARA_WIDTH);
  node.paint.color = Color::new_srgba(1.0, 1.0, 1.0, 1.0);
  node
}

/// Warm the text atlas for the HUD's style ahead of its use, so the frame
/// that first draws the overlay does not stop to make its cells. Called once
/// the font set is registered, and again after every reset, which voids the
/// atlas's warm-ups.
pub fn warm(platform: &PlatformContext) {
  let style = hud_text().run_style();
  let fonts = platform.glyphs();
  let Some(face) = fonts.resolve(&style.font_family) else { return };
  platform.text_atlas().request_warm(WarmRequest {
    face,
    size: style.font_size,
    weight: weight_value(style.font_weight),
    stretch: style.font_stretch,
    italic: style.font_style == FontStyle::Italic,
    text: None,
  });
}

/// The dev-session fact shown on the overlay's first line: the client is
/// connected to a dev server (which controls it), its user input is muted by
/// that server (a mute implies the connection: it clears on disconnect), or
/// a pushed version is being installed, which can take seconds when it
/// precompiles wasm modules and would otherwise read as a hang.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Badge {
  Connected,
  Muted,
  Installing,
}

/// Build the overlay declaration the raster thread composites over every
/// finished frame (after any window shader pass): a display list drawn at
/// the origin, plus the window-space rectangle it belongs in (physical
/// pixels - `scale` is the display scale the app's own lists are built
/// with, so the text lays out in the same logical coordinates `safe_area`
/// is in). The first line is the badge (if any) and, with `hud` on, the
/// stats HUD's headline figures, FPS last; the rest of the HUD follows only
/// with `hud` on. `gpu_pct` is the GPU share over the last second's recorded
/// frames (frame_history::RasterRates::gpu_share_pct, the same figure the
/// stats query reports), None when there is no GPU timing source or no
/// frame changed the picture.
///
/// The text is rasterized by the glyph engine (`Text::rasterize`) into a
/// texture the display list composites; the image comes back with the
/// declaration so the caller keeps it alive and hands it in as `previous`
/// next time, which re-renders into the same texture while the size
/// holds. None when nothing could be drawn (no fonts yet).
#[allow(clippy::too_many_arguments)]
/// The overlay's text for the figures `s`: the badge, the figures when
/// `hud`, the FPS. Composed apart from the raster so a refresh whose text
/// is unchanged (a badge over an idle app, the same FPS) is skipped.
pub fn text(s: &StatsSnapshot, gpu_pct: Option<f32>, hud: bool, badge: Option<Badge>) -> String {
  let mut text = String::new();
  match badge {
    Some(Badge::Connected) => text.push_str("CONN "),
    Some(Badge::Muted) => text.push_str("MUTED "),
    Some(Badge::Installing) => text.push_str("INSTALLING "),
    None => {}
  }
  if hud {
    text.push_str(&format!("{:.0}% CPU {:.0} MEM ", s.cpu_pct, s.mem_bytes as f32 / MIB));
  }
  text.push_str(&format!("{} FPS", s.fps));
  if hud {
    push_hud_lines(&mut text, s, gpu_pct);
  }
  text
}

/// The overlay for `text` (see `text`): rasterized at `scale`, placed in
/// `safe_area`, re-rendered into `previous` while its size holds.
pub fn build(
  text: String,
  platform: &PlatformContext,
  alloy: &alloy::Context,
  previous: Option<&TextImage>,
  safe_area: Rect,
  scale: f32,
) -> Option<(alloy::Overlay, TextImage)> {
  let mut node = hud_text();
  node.set_plain_text(text);
  let image = node.rasterize(platform, alloy, PARA_WIDTH, scale, previous)?;

  // Darkening backdrop so the white text stays legible over light content,
  // drawn at the origin: placement travels as the declaration's rectangle.
  // The text is right-aligned in PARA_WIDTH: its ink box says where the
  // lines landed, and the backdrop fits that ink one pad around.
  let ink = image.ink;
  let w = ink.size.width + PAD * 2.0;
  let h = ink.size.height + PAD * 2.0;
  let mut b = DisplayListBuilder::new(None);
  b.scale(scale, scale);
  let mut bg_paint = Paint::default();
  bg_paint.set_color(Color::new_srgba(0.0, 0.0, 0.0, BACKDROP_ALPHA));
  b.draw_rect(&Rect::new(Point::new(0.0, 0.0), Size::new(w, h)), &bg_paint);
  // The text origin sits where its ink lands one pad inside the backdrop;
  // the image's box (ink slack included) is placed relative to that origin
  // and mapped 1:1 onto its texels.
  let origin = Point::new(PAD - ink.origin.x, PAD - ink.origin.y);
  let src = Rect::new(Point::zero(), Size::new(image.rect.size.width * scale, image.rect.size.height * scale));
  let dst = Rect::new(origin + image.rect.origin.to_vector(), image.rect.size);
  b.draw_texture_rect(&image.texture, &src, &dst, TextureSampling::Linear, None);

  // Same anchor the in-tree overlay drew at: the text's right edge INSET
  // logical px inside the safe area's top-right corner, its top INSET
  // below the safe area's top, the backdrop one pad around it.
  let win_x = safe_area.origin.x + safe_area.size.width - INSET - ink.size.width - PAD;
  let win_y = safe_area.origin.y + INSET - PAD;
  let dl = b.build()?;
  let overlay = alloy::Overlay {
    dl,
    x: (win_x * scale).round() as i32,
    y: (win_y * scale).round() as i32,
    width: (w * scale).ceil() as u32,
    height: (h * scale).ceil() as u32,
  };
  Some((overlay, image))
}

/// The stats HUD's remaining lines, appended under the first line.
fn push_hud_lines(text: &mut String, s: &StatsSnapshot, gpu_pct: Option<f32>) {
  let paint_stats = s.paint;
  // Each timing is shown as a share of the measured frame period. Every
  // figure and frame_ms are smoothed the same way on the same cadence (the
  // phases record zero on reused and skipped frames, see Stats::record_frame),
  // so a share stays within 100%. Shares sum to ~100% when CPU-bound; less
  // means idle or GPU-bound headroom. A share is relative to the current
  // frame, so one phase shrinks when another grows. JS = the frame's JS
  // (timers, rAF, onFrame + flush);
  // LAY/PNT/PST/HOV = native draw phases; all four read 0% for an app whose
  // tree is static, however much its pixels change. SET is a raw count
  // (setProperty writes/frame), not a share.
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
  // GPU execution (window draw plus shader passes) as a share of the PRESENT
  // interval, not of frame_ms like the four above: the demand gate makes
  // those differ by exactly the frames it skips (a settled app presenting
  // once a second read GPU 50% at 1.3% busy). Computed by the frame history
  // over the last second's recorded frames, the one computation the stats
  // query also reports (a second one here, over its own sample marks, read
  // 0% while the query said 16% - okf/done/stats-overlay-gpu-share.md).
  // Hidden without a GPU timing source, and while no frame changes the
  // picture.
  if let Some(gpu_pct) = gpu_pct {
    text.push_str(&format!("\nGPU {:.0}%", gpu_pct));
  }
  // Demand-gate savings/sec: frames served from the cached display list
  // (reuse) and frames skipped entirely (skip). Hidden when the gate saved
  // nothing this second - every frame a full rebuild, which FPS already shows.
  if s.reused + s.skipped > 0 {
    text.push_str(&format!("\n{} reuse {} skip", s.reused, s.skipped));
  }
  // Repaint boundaries this frame: reused+recorded. Hidden when the app
  // declares none, and on frames with no paint walk (display-list reuse),
  // since the counts are the frame's own.
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
  // Glass panels re-filtered ahead of a fading group this frame; hidden
  // while no such fade runs.
  if paint_stats.backdrops_prepainted > 0 {
    text.push_str(&format!("\n{} GLASS", paint_stats.backdrops_prepainted));
  }
  // The last rebuild's display-list ops: draws, clips and save layers
  // (DRW/CLP/LYR), then the text layers the glyph pass rasterized and the
  // paints that leave a tiled GPU's cheap path, non-source-over blends and
  // gradients (TXT, BLD/GRD), each shown only when any.
  let ops = s.counters;
  if ops.draws > 0 {
    text.push_str(&format!("\n{} DRW {} CLP {} LYR", ops.draws, ops.clips, ops.save_layers));
  }
  if ops.text_layers > 0 {
    text.push_str(&format!("\n{} TXT", ops.text_layers));
  }
  if ops.blends + ops.gradients > 0 {
    text.push_str(&format!("\n{} BLD {} GRD", ops.blends, ops.gradients));
  }
  // Textures currently held in the registry (GL/Impeller texture pairs in use).
  if s.textures > 0 {
    text.push_str(&format!("\n{} TEX", s.textures));
  }
}
