mod decoration;
pub mod glyphs;
pub(crate) mod gradient;
pub mod layout;
mod runs;
mod shape;
mod words;

pub use decoration::{Underline, UnderlineMetrics};
pub use glyphs::Fallback;
pub use runs::{RunOverrides, RunStyle, Span, TextRun, ATOM_CHAR};
pub use shape::{prepare_units, PreparedRun, PreparedUnit};
pub use words::{CaretStop, ShapedWord, WordCache};

use crate::gpu::{GlyphGroup, GlyphQuad};
use crate::impellers::{
  DisplayListBuilder, FontStyle, FontWeight, Point, Rect, Size, TextAlignment, Texture, TextureSampling,
};
use crate::rendertree::text::glyphs::{split_phase, weight_value, Face, Hint, StyleKey};
use crate::rendertree::text::layout::{PlacedRun, Run, Wrap};
use crate::rendertree::{
  grid, Bounded, BuildContext, Buildable, Damage, Element, ElementKind, Measurable, MeasureContext, PaintState,
  PlatformContext,
};
use crate::Context;
use shape::OwnedCache;
use std::cell::{Cell, RefCell};
use taffy::{AvailableSpace, Display, Style};

// Shaping bound: at most this many widths cached per text node. A layout pass
// probes the intrinsic width (f32::MAX) plus the resolved width, and paint
// asks for the content width, so a handful covers a frame; oldest is evicted.
const MAX_CACHED_WIDTHS: usize = 4;

// How far (px) a piece may sit from the end of the piece before it and
// still draw with it as one paragraph: the breaker's pen is a running f32
// sum of advances, so adjacent pieces differ from it by rounding only.
const LINE_JOIN_EPSILON: f32 = 0.01;

/// What happens to text cut off by `max_lines`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TextOverflow {
  #[default]
  Clip,
  /// The string drawn at the end of the last line in the paragraph's default
  /// style, the last line trimmed until it fits.
  Ellipsis(String),
}

/// What happens to a wrap unit wider than its line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverflowWrap {
  /// Keep it whole and let it overflow (CSS's default).
  Normal,
  /// Split it at grapheme boundaries, only when it does not fit alone.
  #[default]
  Anywhere,
}

/// Where a detached text's `x` sits on its lines (SVG text-anchor). Anchored
/// text is point-placed rather than boxed: with no `w` it shapes at its
/// natural width (no wrap), its lines align to the anchor's side unless
/// textAlign says otherwise, and the extent is shifted so the anchor lands
/// on `x`. Owned path only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnchor {
  Start,
  Middle,
  End,
}

impl TextAnchor {
  // The share of the extent that sits left of x.
  fn fraction(self) -> f32 {
    match self {
      TextAnchor::Start => 0.0,
      TextAnchor::Middle => 0.5,
      TextAnchor::End => 1.0,
    }
  }

  fn alignment(self) -> TextAlignment {
    match self {
      TextAnchor::Start => TextAlignment::Left,
      TextAnchor::Middle => TextAlignment::Center,
      TextAnchor::End => TextAlignment::Right,
    }
  }
}

// The engine text defaults, shared by Default and the null-reset paths in
// set_font_size / set_font_weight. Regular since 2026-10-08: the Medium
// default compensated Impeller's grayscale blending, which thinned light
// text on 1x displays; the glyph pass blends with DirectWrite's recipe now
// (okf/done/dpi-aware-default-font-weight.md).
pub const DEFAULT_FONT_SIZE: f32 = 20.0;
pub const DEFAULT_FONT_WEIGHT: FontWeight = FontWeight::Regular;
/// CSS font-stretch's normal: the width axis at 100 percent.
pub const DEFAULT_FONT_STRETCH: f32 = 100.0;

#[derive(Clone, Debug)]
pub struct Text {
  // Concatenation of every span's text; what search and snapshots read.
  pub computed_text: String,
  // The same text as styled runs, in order: each span leaf's text with the
  // overrides layered along its span ancestry. Resolved against this Text's
  // own fields at shape time, so a `<text>` prop change needs no resync.
  pub runs: Vec<TextRun>,
  pub font_family: String,
  pub font_size: f32,
  pub font_style: FontStyle,
  pub font_weight: FontWeight,
  // The width axis percentage (CSS font-stretch), 100 normal.
  pub font_stretch: f32,
  // Extra advance after every character, px (CSS letter-spacing).
  pub letter_spacing: f32,
  // None takes the anchor's side, else left (see alignment()).
  pub text_alignment: Option<TextAlignment>,
  // Detached-only: point-places the text at x instead of boxing it.
  pub anchor: Option<TextAnchor>,
  // 0 = unlimited.
  pub max_lines: u32,
  pub text_overflow: TextOverflow,
  pub overflow_wrap: OverflowWrap,
  // First-line indent in pixels; negative hangs: the first line starts at 0
  // and every following line is indented by the magnitude. Owned path only.
  pub text_indent: f32,
  // Owned path only.
  pub text_wrap: Wrap,
  pub line_height: f32,
  // Underline (CSS text-decoration: underline) in the run's paint. Offset
  // (baseline to the stroke's top) and thickness in pixels; None takes the
  // font's own metrics (see text::decoration). Paint-only, owned path only.
  pub underline: bool,
  pub underline_offset: Option<f32>,
  pub underline_thickness: Option<f32>,
  // Paint-time box overrides, mirroring Rectangle's x/y/w/h. x/y offset the
  // drawn paragraph. w overrides the shaping (wrap) width, which otherwise
  // falls back to the inherited layout size - detached text has no box of its
  // own, so give it a w for an unwrapped natural line. h cannot affect shaping
  // (paragraph height falls out of the text); it only feeds the reported
  // bounds. None of these affect layout.
  pub x: Option<f32>,
  pub y: Option<f32>,
  pub w: Option<f32>,
  pub h: Option<f32>,
  pub paint: PaintState,
  // The run metrics per wrap-unit piece (measured once per key through the
  // shared word cache; the shaped words themselves live only in that
  // cache) plus the line layouts derived from them, keyed by width. Shaping
  // dominates measure/build cost and properties are written directly from
  // several places, so validity is checked by fingerprint (ParaKey) instead
  // of setter hooks. Interior-mutable: measure and build take &self.
  owned: RefCell<OwnedCache>,
  // The raster of this text (see TextLayer).
  layer: RefCell<Option<TextLayer>>,
  // The composite scale as the builds saw it (see ScaleSeen): a scale that
  // held over the rest looks is at rest (build_layer).
  scale_seen: Cell<Option<ScaleSeen>>,
  // The last build kept its raster under a scale that moved and asked for
  // a look at the next frame (build_layer); the walk registers it.
  scale_wait: Cell<bool>,
  // The raster scale the last build chose, whether or not its pass ran.
  chosen_scale: Cell<Option<f32>>,
}

impl Default for Text {
  fn default() -> Self {
    Self {
      computed_text: String::new(),
      runs: Vec::new(),
      font_family: "sans".to_string(),
      font_size: DEFAULT_FONT_SIZE,
      font_style: FontStyle::Normal,
      font_weight: DEFAULT_FONT_WEIGHT,
      font_stretch: DEFAULT_FONT_STRETCH,
      letter_spacing: 0.0,
      text_alignment: None,
      anchor: None,
      max_lines: 0,
      text_overflow: TextOverflow::default(),
      overflow_wrap: OverflowWrap::default(),
      text_indent: 0.0,
      text_wrap: Wrap::default(),
      line_height: 0.0,
      underline: false,
      underline_offset: None,
      underline_thickness: None,
      x: None,
      y: None,
      w: None,
      h: None,
      paint: PaintState::default(),
      owned: RefCell::new(OwnedCache::default()),
      layer: RefCell::new(None),
      scale_seen: Cell::new(None),
      scale_wait: Cell::new(false),
      chosen_scale: Cell::new(None),
    }
  }
}

// One drawn run of a line: a line's adjacent pieces in one style whose
// positions are the sum of their advances, joined into one string (see
// `Text::line_runs`), and where it starts.
struct LineRun {
  text: String,
  style: usize,
  x: f32,
  y: f32,
}

/// The raster a `<text>` retains (okf/plans/text-own-rasterizer.md, stage
/// 2): its glyphs drawn by the glyph pass from the text atlas into a
/// texture of its painted box, composited as one quad. Validated by
/// fingerprint like the shaping cache beside it: the prepared cache's
/// generation, the shaping width, the scale, and the texel size. Holds
/// pixels, never atlas cells, so cells may move or go beneath it. Released
/// once the text goes unbuilt for `TEXT_LAYER_RELEASE_FRAMES` (scrolled
/// out, culled, hidden), so a long document holds the layers of what it
/// shows, not of all it has.
#[derive(Clone)]
struct TextLayer {
  texture: Texture,
  tex_w: u32,
  tex_h: u32,
  /// The box the pixels cover, relative to the text's origin (logical px).
  rect: Rect,
  scale: f32,
  generation: u64,
  width: f32,
  /// The platform's text rendering generation the layer was drawn under.
  rendering: u64,
  /// Whether the cells were hinted: a raster made for a scale the text
  /// rests at is, one made mid-motion at an in-between scale is not (see
  /// build_layer), and is re-made hinted once the scale rests.
  hinted: bool,
  /// The text atlas frame this layer was last composited at.
  used: u64,
}

/// A text rasterized outside a tree walk (see `Text::rasterize`): the
/// texture and its size in texels, the box it covers relative to the text's
/// origin (logical px), and the placed ink's box.
pub struct TextImage {
  pub texture: Texture,
  pub tex_w: u32,
  pub tex_h: u32,
  pub rect: Rect,
  pub ink: Rect,
}

// Frames a text may go unbuilt before its layer texture is released: two
// seconds at 60 Hz, the text atlas's own eviction age, so a screen that
// comes right back keeps its pixels.
const TEXT_LAYER_RELEASE_FRAMES: u64 = 120;

impl std::fmt::Debug for TextLayer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "TextLayer({}x{} at {})", self.tex_w, self.tex_h, self.scale)
  }
}

// The ratio (either way) between the composite scale and the raster scale
// past which a layer re-rasterizes even while the scale is moving: the cap
// on how soft a stretched raster may get mid-animation (Chrome's pinch
// rule re-rasters at 2).
const LAYER_SCALE_MOTION_BOUND: f32 = 1.5;
// Two composite scales within this fraction of each other are the same
// scale: a frame's matrix composition noise, never a step of an animation.
const SCALE_HELD_EPSILON: f32 = 1e-5;
// Consecutive looks a composite scale must hold before it is at rest: 50 ms
// at 60 Hz, past the gap of a writer at half the refresh rate, so an
// animation driven at 30 Hz does not read as resting between its writes.
const SCALE_REST_LOOKS: u32 = 3;

/// A node's observation of its composite scale: the scale at its last
/// look, the frame of that look, and how many consecutive looks it has
/// held for (`scale_observed`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleSeen {
  pub scale: f32,
  pub frame: u64,
  pub held: u32,
}

impl ScaleSeen {
  /// Whether the scale has held long enough to count as at rest.
  pub fn at_rest(&self) -> bool {
    self.held >= SCALE_REST_LOOKS
  }
}

/// The observation after a look at `now` in `frame`, given the previous
/// one: the hold grows by one when the look follows the previous frame
/// and the scale is the same, and starts over when the scale moved or the
/// node was not looked at last frame (unknown: wait a frame).
pub(crate) fn scale_observed(prev: Option<ScaleSeen>, now: f32, frame: u64) -> ScaleSeen {
  let held = match prev {
    Some(prev) if prev.frame + 1 == frame && scale_held(prev.scale, now) => prev.held + 1,
    _ => 0,
  };
  ScaleSeen { scale: now, frame, held }
}

/// What a raster made at one scale does under another (`raster_scale`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RasterScale {
  /// The scale is the raster's: the raster stays.
  Keep,
  /// The scale moved and has not held yet: the raster stays, stretched,
  /// and the node asks for a look at the next frame.
  Wait,
  /// The scale held at a new value, or ran past the motion bound: a new
  /// raster at the composite scale.
  Rerasterize,
}

/// The raster hysteresis rule (okf/done/text-layer-motion.md), shared by
/// the text layer, a recording boundary holding text layers and a snapshot
/// boundary (through `raster_density`): a raster made at `have` under a
/// composite scale of `now`, `at_rest` when `now` held over the rest
/// looks. The raster scale follows rest, as browsers treat a transform
/// animation: a zoom composites the existing raster stretched and
/// re-rasterizes once where it comes to rest, for exactly that scale; a
/// breathing scale never re-rasterizes; only a drift past the motion
/// bound re-rasterizes in flight.
pub(crate) fn raster_scale(have: f32, now: f32, at_rest: bool) -> RasterScale {
  if !(have > 0.0 && now > 0.0 && have.is_finite() && now.is_finite()) {
    return RasterScale::Keep;
  }
  if (now / have).max(have / now) > LAYER_SCALE_MOTION_BOUND {
    return RasterScale::Rerasterize;
  }
  if scale_held(have, now) {
    return RasterScale::Keep;
  }
  if at_rest {
    RasterScale::Rerasterize
  } else {
    RasterScale::Wait
  }
}

/// The density a raster is made or kept at, and whether the node waits for
/// rest: `have` the raster's scale (None before the first), `now` the
/// composite scale, `factor` where the running scale transitions on the
/// chain take it (`BuildContext::scale_target`, 1 with none), `at_rest`
/// whether `now` held over the rest looks. With a transition headed up,
/// the raster is made for its end at once and the motion minifies it, as
/// Chrome rasters a known animation at its larger end; headed down, the
/// raster is kept and minified, and re-made at the resting scale.
pub(crate) fn raster_density(have: Option<f32>, now: f32, factor: f32, at_rest: bool) -> (f32, bool) {
  let ahead = (factor > 1.0 && factor.is_finite()).then(|| now * factor);
  let Some(have) = have else {
    return (ahead.unwrap_or(now), false);
  };
  let up = ahead.filter(|&end| end > have && !scale_held(have, end));
  match raster_scale(have, now, at_rest) {
    RasterScale::Rerasterize => (ahead.map_or(now, |end| end.max(now)), false),
    RasterScale::Keep => (up.unwrap_or(have), false),
    RasterScale::Wait => match up {
      Some(end) => (end, false),
      None => (have, true),
    },
  }
}

/// Whether two composite scales observed a frame apart are the same scale.
pub(crate) fn scale_held(a: f32, b: f32) -> bool {
  (a - b).abs() <= b.abs() * SCALE_HELD_EPSILON
}

// The layer's raster density: the device pixels per logical pixel of the
// target the layer is drawn into, from the walk's grid map (the display
// scale times the ancestors' scale; the display scale alone past a 3d
// transform, where the map is gone).
fn layer_scale(ctx: &BuildContext<'_>) -> f32 {
  grid::scale(&ctx.grid).unwrap_or_else(|| ctx.platform.display_scale())
}

impl Buildable for Text {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
    // Lines wrap at and start from the content box - the box taffy measured
    // the text against, and the inset place_atoms already applies to inline
    // atoms (okf/done/padding-box-divergence.md). x/y are detached-only
    // geometry, where the content box is the whole frame at origin zero.
    let mut origin =
      Point::new(ctx.content.origin.x + self.x.unwrap_or(0.0), ctx.content.origin.y + self.y.unwrap_or(0.0));
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(ctx.platform, &mut owned);
    let width = self.shaping_width(&owned, ctx.content.size.width);
    origin.x += self.anchor_shift(width);
    let index = self.owned_layout(ctx.platform, &mut owned, width);
    let owned = &*owned;
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    let styles = self.run_styles();
    let line_runs = self.line_runs(owned, index);
    self.build_layer(ctx, builder, origin, owned, index, &styles, &line_runs);
    // CSS decorating boxes: the text's underline is one line in its own
    // style under everything (atoms excepted); a span's underline is its
    // own line in the span's style. Both may cover a run.
    let fonts = ctx.platform.glyphs();
    let underline_metrics = |family: &str| {
      fonts.resolve(family).and_then(|id| fonts.face(id)).map_or(UnderlineMetrics::DEFAULT, Face::underline)
    };
    let ink_of = |placed: &PlacedRun| runs[placed.run].run.metrics.ink_width;
    if self.underline {
      let style = self.run_style();
      let underline = Underline::resolve(
        underline_metrics(&style.font_family),
        style.font_size,
        self.underline_offset,
        self.underline_thickness,
      );
      decoration::draw_underlines(
        builder,
        origin,
        layout,
        |placed| (!runs[placed.run].atom).then_some((underline, &style.paint)),
        ink_of,
      );
    }
    if self.runs.iter().any(|r| r.overrides.underline == Some(true)) {
      decoration::draw_underlines(
        builder,
        origin,
        layout,
        |placed| {
          let shaped = &runs[placed.run];
          if shaped.atom {
            return None;
          }
          let overrides = &self.runs[shaped.style].overrides;
          if overrides.underline != Some(true) {
            return None;
          }
          let style = &styles[shaped.style];
          let underline = Underline::resolve(
            underline_metrics(&style.font_family),
            style.font_size,
            overrides.underline_offset.or(self.underline_offset),
            overrides.underline_thickness.or(self.underline_thickness),
          );
          Some((underline, &style.paint))
        },
        ink_of,
      );
    }
  }
}

impl Measurable for Text {
  fn measure(&self, ctx: &MeasureContext) -> Size {
    crate::rendertree::counters::note_measure_call();
    if let (Some(w), Some(h)) = (ctx.known.width, ctx.known.height) {
      return Size::new(w, h);
    }
    self.measure_owned(ctx)
  }
}

impl Bounded for Text {
  // A box-level answer on purpose: `fallback` is taken raw, with no content
  // inset. The bounding-box path (tree::compute_corners) passes the layout box
  // and wants the element's box for laid-out text; a detached text, which has
  // no box, answers with detached_bounds instead. Line-level ink (content
  // origin, wrap width) is painted_extent's job
  // (okf/done/padding-box-divergence.md).
  fn local_bounds(&self, fallback: Size) -> Rect {
    Rect::new(
      Point::new(self.x.unwrap_or(0.0) + self.w.map_or(0.0, |w| self.anchor_shift(w)), self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(fallback.width), self.h.unwrap_or(fallback.height)),
    )
  }
}

impl Text {
  // The drawn runs of the layout at `index`: a line's adjacent pieces in
  // one style whose positions are the sum of their advances draw as ONE
  // run of their joined text: the breaker placed them exactly where the
  // shaper puts them in one string, so the joined draw lands the glyphs
  // where the per-piece draws would, and the line costs one cache lookup
  // and one draw instead of one per word. Where placement is not the sum
  // of advances the pieces stay apart: a justified line (its gaps are
  // spread), a style change, an atom, and a layout re-split at graphemes
  // (overflowWrap), whose pieces are letters the shaper would kern and
  // ligate back together. Hit testing and carets keep reading the
  // per-piece metrics, and this is as LTR-only as the breaker: bidi
  // becomes an input to both. The ellipsis, when the layout placed one,
  // comes last.
  fn line_runs(&self, owned: &OwnedCache, index: usize) -> Vec<LineRun> {
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    let joinable = owned.layouts[index].runs.is_none();
    let mut out = Vec::new();
    for line in &layout.lines {
      // The run being joined: its text, style, origin and where its ink
      // ends (where the next piece must start to join it).
      let mut pending: Option<(String, usize, f32, f32, f32)> = None;
      for placed in &layout.runs[line.first..line.end] {
        let shaped = &runs[placed.run];
        if let Some((text, style, _, y, end)) = &mut pending {
          let adjacent = (*end - placed.x).abs() <= LINE_JOIN_EPSILON && (*y - placed.y).abs() <= LINE_JOIN_EPSILON;
          if joinable && !shaped.atom && *style == shaped.style && adjacent {
            text.push_str(&shaped.text);
            *end = placed.x + shaped.run.metrics.advance;
            continue;
          }
          let (text, style, x, y, _) = pending.take().expect("checked above");
          out.push(LineRun { text, style, x, y });
        }
        if !shaped.atom {
          let end = placed.x + shaped.run.metrics.advance;
          pending = Some((shaped.text.clone(), shaped.style, placed.x, placed.y, end));
        }
      }
      if let Some((text, style, x, y, _)) = pending {
        out.push(LineRun { text, style, x, y });
      }
    }
    if let (Some((x, y)), Some(ellipsis)) = (layout.ellipsis, owned.ellipsis.as_ref()) {
      out.push(LineRun { text: ellipsis.text.clone(), style: ellipsis.style, x, y });
    }
    out
  }

  // Draw the text through its layer: the runs' glyphs as quads from the
  // text atlas, rasterized by the glyph pass into a texture of the text's
  // painted box at the raster scale, and that texture composited as one
  // quad. The layer is kept while its inputs hold (see TextLayer). The
  // raster scale follows the composite scale at rest and holds while it
  // moves (`raster_density`): a text under a scale animation composites
  // its existing raster stretched until the scale has held, then
  // re-rasterizes once, or rasters for a known end at once; a text that
  // decides to wait asks for a look at the next frame (`scale_wait`, which
  // the walk registers with the tree).
  #[allow(clippy::too_many_arguments)]
  fn build_layer(
    &self,
    ctx: &mut BuildContext<'_>,
    builder: &mut DisplayListBuilder,
    origin: Point,
    owned: &OwnedCache,
    index: usize,
    styles: &[RunStyle],
    line_runs: &[LineRun],
  ) {
    let composite = layer_scale(ctx);
    // At rest when the scale held over the rest looks, or when an enclosing
    // recording decided so for its record walk.
    let seen = scale_observed(self.scale_seen.get(), composite, ctx.frame);
    self.scale_seen.set(Some(seen));
    let at_rest = seen.at_rest() || ctx.scale_at_rest;
    let mut layer = self.layer.borrow_mut();
    let (scale, wait) = raster_density(layer.as_ref().map(|l| l.scale), composite, ctx.scale_target, at_rest);
    self.scale_wait.set(wait);
    self.chosen_scale.set(Some(scale));
    if wait {
      ctx.platform.request_frame();
    }
    // Hinted when made for a scale the text rests at: at rest, at the
    // display scale (every static text), or at a running transition's end,
    // which it rests at next. A raster made mid-motion at an in-between
    // scale is unhinted, so its x-height does not pop between rows as the
    // size moves; a kept raster keeps its hinting while the scale moves and
    // is re-made hinted once it rests.
    let display_scale = ctx.platform.display_scale();
    let at_end = ctx.scale_target != 1.0 && scale_held(scale, composite * ctx.scale_target);
    let hinted = at_rest || scale == display_scale || at_end;
    let Some((rect, tex_w, tex_h)) = Self::layer_geometry(owned, index, scale) else {
      return;
    };
    ctx.text_layers += 1;
    let width = owned.layouts[index].width;
    let content = Size::new(width, owned.layouts[index].layout.height);
    let frame = ctx.frame;
    let rendering = ctx.platform.text_rendering_generation();
    let current = layer.as_ref().is_some_and(|l| {
      l.generation == owned.generation
        && l.rendering == rendering
        && l.width == width
        && l.scale == scale
        && (l.hinted == hinted || !at_rest)
        && l.tex_w == tex_w
        && l.tex_h == tex_h
    });
    if !current {
      // The frame's count of layers the walk rasterized (paintOps.textLayers);
      // the stats overlay's own raster, outside any walk, is not one.
      crate::rendertree::counters::note_text_layer_drawn();
      let into = layer.as_ref().filter(|l| l.tex_w == tex_w && l.tex_h == tex_h).map(|l| l.texture.clone());
      match self.draw_layer(
        ctx.platform,
        ctx.alloy,
        styles,
        line_runs,
        content,
        rect,
        scale,
        hinted,
        tex_w,
        tex_h,
        into.as_ref(),
      ) {
        Ok(Some(texture)) => {
          *layer = Some(TextLayer {
            texture,
            tex_w,
            tex_h,
            rect,
            scale,
            generation: owned.generation,
            width,
            rendering,
            hinted,
            used: frame,
          });
        }
        // No atlas texture: the GPU refused it (logged there); nothing to
        // draw this frame.
        Ok(None) => return,
        Err(e) => {
          log::warn!("[text] layer rasterization failed: {e}");
          layer.take();
          return;
        }
      }
    }
    let layer = layer.as_mut().expect("built above");
    layer.used = frame;
    // The content occupies the top-left rect * scale pixels of the
    // ceil-padded texture; mapping exactly that region onto the logical box
    // keeps the composite pixel-exact.
    let src =
      Rect::new(Point::zero(), Size::new(layer.rect.size.width * layer.scale, layer.rect.size.height * layer.scale));
    // The glyph quads snap to pixel rows and subpixel phases inside the
    // layer, so the layer's quad is a painted box like any other: its
    // origin (the text origin plus the slack, fractional) lands on a whole
    // device pixel, and the text moves by under a pixel with it.
    let mut dst = Rect::new(origin + layer.rect.origin.to_vector(), layer.rect.size);
    if let Some(shift) = grid::shift(&ctx.grid, dst.origin) {
      dst.origin += shift;
    }
    crate::rendertree::counters::note_draw();
    builder.draw_texture_rect(&layer.texture, &src, &dst, TextureSampling::Linear, None);
  }

  /// This text drawn on its own, outside a tree walk (the stats HUD): laid
  /// out at `width`, rasterized at `scale` device pixels per logical pixel,
  /// re-rendered into `previous`'s texture while the texel size matches.
  /// None when nothing could be drawn (no fonts, no atlas texture, a pass
  /// failure, which is logged).
  pub fn rasterize(
    &self,
    platform: &PlatformContext,
    alloy: &Context,
    width: f32,
    scale: f32,
    previous: Option<&TextImage>,
  ) -> Option<TextImage> {
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(platform, &mut owned);
    let index = self.owned_layout(platform, &mut owned, width);
    let owned = &*owned;
    let (rect, tex_w, tex_h) = Self::layer_geometry(owned, index, scale)?;
    let content = Size::new(width, owned.layouts[index].layout.height);
    let styles = self.run_styles();
    let line_runs = self.line_runs(owned, index);
    let into = previous.filter(|p| p.tex_w == tex_w && p.tex_h == tex_h).map(|p| &p.texture);
    let hinted = scale == platform.display_scale();
    let texture =
      match self.draw_layer(platform, alloy, &styles, &line_runs, content, rect, scale, hinted, tex_w, tex_h, into) {
        Ok(drawn) => drawn?,
        Err(e) => {
          log::warn!("[text] rasterization failed: {e}");
          return None;
        }
      };
    Some(TextImage { texture, tex_w, tex_h, rect, ink: Self::ink_box(owned, index) })
  }

  // The box a layer of the layout at `index` covers, relative to the text
  // origin (logical px): the lines plus a line height of slack on every
  // side, since ink overhangs its line box (italics, descenders), as
  // painted_extent reckons; and its size in texels at `scale`. None when
  // there is nothing to draw into.
  fn layer_geometry(owned: &OwnedCache, index: usize, scale: f32) -> Option<(Rect, u32, u32)> {
    let layout = &owned.layouts[index].layout;
    let width = owned.layouts[index].width;
    let slack = layout.lines.iter().map(|l| l.height).fold(0.0, f32::max);
    let rect = Rect::new(
      Point::new(-slack, -slack),
      Size::new(width.max(layout.width) + 2.0 * slack, layout.height + 2.0 * slack),
    );
    let tex_w = (rect.size.width * scale).ceil() as u32;
    let tex_h = (rect.size.height * scale).ceil() as u32;
    if tex_w == 0 || tex_h == 0 || !scale.is_finite() {
      return None;
    }
    Some((rect, tex_w, tex_h))
  }

  // Rasterize `line_runs` through the glyph pass into a texture of `tex_w`
  // x `tex_h` texels covering `rect` at `scale` (or `into` an earlier one
  // of that size), `hinted` as the platform's text hinting says or not at
  // all, `content` being the text's own box (what a box gradient resolves
  // against). Ok(None) when there is no atlas texture (the GPU refused it,
  // logged there), Err when the pass failed.
  #[allow(clippy::too_many_arguments)]
  fn draw_layer(
    &self,
    platform: &PlatformContext,
    alloy: &Context,
    styles: &[RunStyle],
    line_runs: &[LineRun],
    content: Size,
    rect: Rect,
    scale: f32,
    hinted: bool,
    tex_w: u32,
    tex_h: u32,
    into: Option<&Texture>,
  ) -> Result<Option<Texture>, String> {
    let (groups, atlas) = self.glyph_quads(platform, alloy, styles, line_runs, content, rect.origin, scale, hinted);
    let Some(atlas) = atlas else {
      return Ok(None);
    };
    let texture = alloy.rasterize_glyphs(groups, atlas, tex_w, tex_h, platform.coverage_policy(), into)?;
    Ok(Some(texture))
  }

  // The placed ink of the layout at `index`, in the text's frame: from the
  // leftmost run's start to the rightmost run's ink end (floats count), the
  // lines' full height. Alignment moves lines within the wrap width, so
  // this is where the ink landed, not the extent.
  fn ink_box(owned: &OwnedCache, index: usize) -> Rect {
    let layout = &owned.layouts[index].layout;
    let runs = owned.runs_for(index);
    let (left, right) = layout
      .runs
      .iter()
      .chain(&layout.floats)
      .fold((f32::MAX, f32::MIN), |(l, r), p| (l.min(p.x), r.max(p.x + runs[p.run].run.metrics.ink_width)));
    let (left, width) = if left <= right { (left, right - left) } else { (0.0, 0.0) };
    Rect::new(Point::new(left, 0.0), Size::new(width, layout.height))
  }

  // The runs' glyphs as quads in layer pixels (the box at `box_origin`
  // relative to the text origin, scaled by `scale`), their cells ensured in
  // the text atlas, grouped by paint: the solid-colored runs together, a
  // gradient run's quads with its gradient resolved against `content` (the
  // text's own box). Returns the groups and the atlas texture (None when
  // the GPU refused it). Each run is shaped through the word cache; a
  // glyph's pen x splits into the pixel its cell snaps to and the subpixel
  // phase the cell is made at, its baseline y snaps to a pixel row. Glyphs
  // are asked of the atlas per style key and phase, since a run's glyphs
  // may come from several faces (fallback) and a cell's style key names
  // its face; every bucket is ensured before any placement is read, so
  // the atlas may grow or evict under the layer without moving a cell the
  // layer has used (see TextAtlas).
  #[allow(clippy::too_many_arguments)]
  fn glyph_quads(
    &self,
    platform: &PlatformContext,
    alloy: &Context,
    styles: &[RunStyle],
    line_runs: &[LineRun],
    content: Size,
    box_origin: Point,
    scale: f32,
    hinted: bool,
  ) -> (Vec<GlyphGroup>, Option<u64>) {
    let fonts = platform.glyphs();
    let mut words = platform.words();
    let mut atlas = platform.text_atlas();
    let display_scale = platform.display_scale();
    let darken_em = platform.text_darken_em();
    // Hinted as the caller decided (build_layer: a raster for a scale the
    // text rests at); a text at the display scale is one at rest on the
    // pixel grid, whose style every other text of its size shares.
    let native = scale == display_scale;
    let hint = if hinted { platform.text_hint() } else { Hint::Off };
    // An untransformed text's style is the one every other text of its
    // size shares, worth warming on first sight; a scaled one is not. Only
    // the run's own face is warmed: a face borrowed for a cluster the
    // primary lacks (fallback) draws those clusters, not the warm-up's
    // ASCII.
    let warm = native;
    // A style's gradient resolved once; None draws the style's solid color.
    let gradients: Vec<Option<crate::gpu::GlyphGradient>> = styles
      .iter()
      .map(|s| s.paint.gradient.as_ref().and_then(|g| gradient::layer_gradient(g, content, box_origin, scale)))
      .collect();
    // A glyph's quad before its placement is known: its cell key, where
    // the quad goes (its pixel x and baseline y) and which group it joins.
    struct Slot {
      style: StyleKey,
      phase: u8,
      id: u16,
      px: i32,
      baseline: f32,
      group: usize,
      color: [f32; 4],
    }
    let mut slots: Vec<Slot> = Vec::new();
    // Per (style key, phase): the glyph ids to ensure, and whether the
    // style is warmed on first sight.
    let mut buckets: Vec<(StyleKey, u8, Vec<u16>, bool)> = Vec::new();
    // The solid group first, then one group per gradient style in
    // first-use order (`graded` maps a style index to its group).
    let mut groups: Vec<GlyphGroup> = vec![GlyphGroup { quads: Vec::new(), gradient: None }];
    let mut graded: Vec<(usize, usize)> = Vec::new();
    for run in line_runs {
      let style = &styles[run.style];
      let Some(word) = words.get_or_shape(&fonts, &run.text, style, Fallback::Registered) else { continue };
      let primary = fonts.resolve(&style.font_family);
      let weight = weight_value(style.font_weight);
      let italic = style.font_style == FontStyle::Italic;
      // The run's color rides on the quad; under a gradient only its alpha
      // counts (the pass reads the color off the ramp).
      let c = style.paint.color;
      let color = [c.red, c.green, c.blue, c.alpha];
      let group = match &gradients[run.style] {
        None => 0,
        Some(gradient) => match graded.iter().find(|(s, _)| *s == run.style) {
          Some((_, group)) => *group,
          None => {
            groups.push(GlyphGroup { quads: Vec::new(), gradient: Some(gradient.clone()) });
            graded.push((run.style, groups.len() - 1));
            groups.len() - 1
          }
        },
      };
      let x0 = (run.x - box_origin.x) * scale;
      let baseline = (run.y + word.metrics.ascent - box_origin.y) * scale;
      for g in &word.glyphs.glyphs {
        let x = x0 + g.x * scale;
        // A fully hinted cell has its stems on whole pixels: one phase, the
        // quad at the nearest pixel.
        let (px, phase) = if hint == Hint::Full { (x.round() as i32, 0) } else { split_phase(x) };
        let key = StyleKey::new(g.face, style.font_size * scale, weight, style.font_stretch, italic, darken_em, hint);
        let warm_face = warm && primary == Some(g.face);
        match buckets.iter_mut().find(|(k, p, _, _)| *k == key && *p == phase) {
          Some(bucket) => {
            bucket.2.push(g.id);
            bucket.3 |= warm_face;
          }
          None => buckets.push((key, phase, vec![g.id], warm_face)),
        }
        slots.push(Slot { style: key, phase, id: g.id, px, baseline: baseline + g.y * scale, group, color });
      }
    }
    for (key, phase, ids, warm_face) in &buckets {
      atlas.ensure(&fonts, *key, *phase, ids, *warm_face);
    }
    let texture = atlas.flush(alloy);
    for slot in slots {
      let Some(p) = atlas.placement(slot.style, slot.phase, slot.id) else { continue };
      if p.width == 0 {
        continue;
      }
      groups[slot.group].quads.push(GlyphQuad {
        dst: [
          (slot.px + p.left) as f32,
          (slot.baseline.round() as i32 - p.top) as f32,
          p.width as f32,
          p.height as f32,
        ],
        src: [p.x as f32, p.y as f32, p.width as f32, p.height as f32],
        color: slot.color,
      });
    }
    if groups[0].quads.is_empty() {
      groups.remove(0);
    }
    (groups, texture)
  }

  /// Whether this text holds a layer texture.
  pub(crate) fn has_layer(&self) -> bool {
    self.layer.borrow().is_some()
  }

  /// Whether the last build kept its raster under a scale that moved and
  /// wants a look at the next frame, to tell rest from motion.
  pub(crate) fn scale_wait(&self) -> bool {
    self.scale_wait.get()
  }

  /// The raster scale the last build chose (a test's read).
  #[cfg(test)]
  pub(crate) fn chosen_scale(&self) -> Option<f32> {
    self.chosen_scale.get()
  }

  /// Release the layer texture once the text has gone unbuilt for the
  /// release age by `frame` (the composite walk asks every text holding
  /// one). Returns whether a layer is still held.
  pub(crate) fn release_stale_layer(&self, frame: u64) -> bool {
    let mut layer = self.layer.borrow_mut();
    if layer.as_ref().is_some_and(|l| frame.saturating_sub(l.used) > TEXT_LAYER_RELEASE_FRAMES) {
      layer.take();
    }
    layer.is_some()
  }

  /// Bounds of a detached text in its own frame (`frame` is the box it
  /// inherits): the laid-out paragraph from the layout the last paint used -
  /// widest line by line stack - with `w`/`h` overriding a side each. Before
  /// a first paint (nothing laid out yet) it is the box answer of
  /// local_bounds.
  pub fn detached_bounds(&self, frame: Size) -> Rect {
    let owned = self.owned.borrow();
    if !owned.key.as_ref().is_some_and(|k| k.matches(self)) {
      return self.local_bounds(frame);
    }
    let width = self.shaping_width(&owned, frame.width);
    let Some(index) = owned.layouts.iter().position(|l| l.width == width) else {
      return self.local_bounds(frame);
    };
    // The placed ink, not the extent: alignment moves lines within the wrap
    // width (Layout::width is measured from the extent's start, before that
    // offset), so a right-aligned boxed text reports where its ink landed.
    // `w` names the box itself and wins over the ink on that axis.
    let ink = Self::ink_box(&owned, index);
    let x = self.x.unwrap_or(0.0) + self.anchor_shift(width);
    Rect::new(
      Point::new(x + if self.w.is_some() { 0.0 } else { ink.origin.x }, self.y.unwrap_or(0.0)),
      Size::new(self.w.unwrap_or(ink.size.width), self.h.unwrap_or(ink.size.height)),
    )
  }

  /// The alignment lines are laid out with: textAlign, else the anchor's
  /// side, else left.
  pub fn alignment(&self) -> TextAlignment {
    self.text_alignment.or_else(|| self.anchor.map(TextAnchor::alignment)).unwrap_or(TextAlignment::Left)
  }

  // The width lines wrap at, shared by paint, bounds and hit testing: `w`,
  // else the natural (unwrapped) width for anchored text, else the content
  // width the text inherits. `owned` must be prepared.
  fn shaping_width(&self, owned: &OwnedCache, content_width: f32) -> f32 {
    if let Some(w) = self.w {
      return w;
    }
    if self.anchor.is_none() {
      return content_width;
    }
    let runs: Vec<Run> = owned.runs.iter().map(|r| r.run).collect();
    layout::max_intrinsic_width(&runs) + self.text_indent.abs()
  }

  // Where the extent's left edge sits relative to x: the anchor's share of
  // the width, leftward. Zero without an anchor.
  fn anchor_shift(&self, width: f32) -> f32 {
    -width * self.anchor.map_or(0.0, TextAnchor::fraction)
  }

  fn measure_owned(&self, ctx: &MeasureContext) -> Size {
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(ctx.platform, &mut owned);
    let runs: Vec<Run> = owned.runs.iter().map(|r| r.run).collect();
    // The intrinsic widths are of the runs alone; an indented line needs its
    // indent on top, else a shrink-to-fit text wraps where it need not.
    let indent = self.text_indent.abs();
    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => (layout::max_intrinsic_width(&runs) + indent).min(w),
      AvailableSpace::MaxContent => layout::max_intrinsic_width(&runs) + indent,
      AvailableSpace::MinContent => layout::min_intrinsic_width(&runs) + indent,
    });
    let height = ctx.known.height.unwrap_or_else(|| {
      let index = self.owned_layout(ctx.platform, &mut owned, width);
      owned.layouts[index].layout.height
    });
    Size::new(width, height)
  }

  /// The box the lines and decorations paint into when built against
  /// `content` (the content box build() reads its origin and width from), in
  /// the text's own frame.
  pub(crate) fn painted_extent(&self, platform: &PlatformContext, content: Rect) -> Option<Rect> {
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(platform, &mut owned);
    let width = self.shaping_width(&owned, content.size.width);
    let origin = Point::new(
      content.origin.x + self.x.unwrap_or(0.0) + self.anchor_shift(width),
      content.origin.y + self.y.unwrap_or(0.0),
    );
    let index = self.owned_layout(platform, &mut owned, width);
    let layout = &owned.layouts[index].layout;
    // Ink overhangs its line box (italics, descenders, an underline pushed
    // below the last line); a line height of slack on every side covers it.
    let slack = layout.lines.iter().map(|l| l.height).fold(0.0, f32::max);
    Some(Rect::new(origin, Size::new(width.max(layout.width), layout.height)).inflate(slack, slack))
  }

  /// Content for a Text outside a tree (measureText, tests): the plain string
  /// as computed_text AND as one unstyled run. The two must agree - shaping
  /// walks the runs to cover the text and indexes them by position, so text
  /// with no runs is a panic, not an empty paragraph. In-tree Texts get both
  /// from their span children (RenderTree::sync_text).
  pub fn set_plain_text(&mut self, text: String) {
    self.runs.clear();
    if !text.is_empty() {
      self.runs.push(TextRun { text: text.clone(), ..TextRun::default() });
    }
    self.computed_text = text;
  }

  // This Text's own fields as the run style every span layers on.
  pub fn run_style(&self) -> RunStyle {
    RunStyle {
      font_family: self.font_family.clone(),
      font_size: self.font_size,
      font_style: self.font_style,
      font_weight: self.font_weight,
      font_stretch: self.font_stretch,
      line_height: self.line_height,
      letter_spacing: self.letter_spacing,
      paint: self.paint.clone(),
    }
  }

  /// Record an atom's measured box, from the layout pass. Returns whether it
  /// changed (a change re-shapes the paragraph via the cache key).
  pub fn set_atom_size(&mut self, node: u64, size: Size) -> bool {
    let Some(run) = self.runs.iter_mut().find(|r| r.node == node && r.atom.is_some()) else {
      return false;
    };
    if run.atom == Some(size) {
      return false;
    }
    run.atom = Some(size);
    true
  }

  /// Where the atoms sit for a layout at `width` (content width), as (node,
  /// top-left) relative to the text's box: the layout pass writes these into
  /// the atoms' computed layouts after the text's own.
  pub fn atom_positions(&self, platform: &PlatformContext, width: f32) -> Vec<(u64, Point)> {
    if self.runs.iter().all(|r| r.atom.is_none()) {
      return Vec::new();
    }
    let mut owned = self.owned.borrow_mut();
    self.prepare_owned(platform, &mut owned);
    let index = self.owned_layout(platform, &mut owned, width);
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    layout
      .runs
      .iter()
      .chain(&layout.floats)
      .filter(|p| runs[p.run].atom)
      .map(|p| (self.runs[runs[p.run].style].node, Point::new(p.x, p.y)))
      .collect()
  }

  /// The span whose text is under `point` (text-local, box `size`), from the
  /// layout the last paint used; None on a miss, or when nothing has been
  /// laid out yet. Atoms are hit as elements, not through here.
  pub fn hit_run(&self, point: Point, content: Rect) -> Option<u64> {
    let owned = self.owned.borrow();
    if !owned.key.as_ref().is_some_and(|k| k.matches(self)) {
      return None;
    }
    // The content box, matching build(): the lines were laid out at the
    // content width and drawn from the content origin, so the lookup and the
    // point both resolve against that box, not the border box
    // (okf/done/padding-box-divergence.md).
    let width = self.shaping_width(&owned, content.size.width);
    // Paint and hit derive the width from the same content_box() arithmetic,
    // so the nearest layout is normally an exact match; the tolerance keeps
    // span hits alive should a caller ever round differently, and a wrap half
    // a pixel off resolves the same runs.
    let index = owned
      .layouts
      .iter()
      .enumerate()
      .filter(|(_, l)| (l.width - width).abs() < 0.5)
      .min_by(|(_, a), (_, b)| {
        (a.width - width).abs().partial_cmp(&(b.width - width).abs()).expect("layout widths are finite")
      })
      .map(|(i, _)| i)?;
    let runs = owned.runs_for(index);
    let layout = &owned.layouts[index].layout;
    let local = point
      - Point::new(
        content.origin.x + self.x.unwrap_or(0.0) + self.anchor_shift(width),
        content.origin.y + self.y.unwrap_or(0.0),
      )
      .to_vector();
    let line = layout.lines.iter().find(|l| local.y >= l.y && local.y < l.y + l.height)?;
    layout.runs[line.first..line.end]
      .iter()
      .find(|p| {
        let shaped = &runs[p.run];
        !shaped.atom && local.x >= p.x && local.x < p.x + shaped.run.metrics.advance
      })
      .map(|p| self.runs[runs[p.run].style].node)
  }

  pub fn set_underline(&mut self, on: Option<bool>) -> Damage {
    self.underline = on.unwrap_or(false);
    Damage::Paint
  }
  pub fn set_underline_offset(&mut self, v: Option<f32>) -> Damage {
    self.underline_offset = v;
    Damage::Paint
  }
  pub fn set_underline_thickness(&mut self, v: Option<f32>) -> Damage {
    self.underline_thickness = v;
    Damage::Paint
  }

  pub fn set_text_overflow(&mut self, v: Option<TextOverflow>) -> Damage {
    self.text_overflow = v.unwrap_or_default();
    Damage::Layout
  }
  pub fn set_overflow_wrap(&mut self, v: Option<OverflowWrap>) -> Damage {
    self.overflow_wrap = v.unwrap_or_default();
    Damage::Layout
  }
  pub fn set_text_indent(&mut self, v: Option<f32>) -> Damage {
    self.text_indent = v.unwrap_or(0.0);
    Damage::Layout
  }
  pub fn set_text_wrap(&mut self, v: Option<Wrap>) -> Damage {
    self.text_wrap = v.unwrap_or_default();
    Damage::Layout
  }

  // Box overrides paint within (or independent of) the layout box, so none of
  // them affect layout.
  pub fn set_x(&mut self, v: Option<f32>) -> Damage {
    self.x = v;
    Damage::Paint
  }
  pub fn set_y(&mut self, v: Option<f32>) -> Damage {
    self.y = v;
    Damage::Paint
  }
  pub fn set_w(&mut self, v: Option<f32>) -> Damage {
    self.w = v;
    Damage::Paint
  }
  pub fn set_h(&mut self, v: Option<f32>) -> Damage {
    self.h = v;
    Damage::Paint
  }

  // All other text properties feed measurement, so every change affects layout.
  // The resolved font family name and FontWeight come in already decoded.
  // None on the numeric props resets to the Default value.
  pub fn set_font_family(&mut self, family: Option<String>) -> Damage {
    self.font_family = family.unwrap_or_else(|| Self::default().font_family);
    Damage::Layout
  }
  pub fn set_font_size(&mut self, v: Option<f32>) -> Damage {
    self.font_size = v.unwrap_or(DEFAULT_FONT_SIZE);
    Damage::Layout
  }
  pub fn set_line_height(&mut self, v: Option<f32>) -> Damage {
    self.line_height = v.unwrap_or(0.0);
    Damage::Layout
  }
  pub fn set_max_lines(&mut self, v: Option<u32>) -> Damage {
    self.max_lines = v.unwrap_or(0);
    Damage::Layout
  }
  // fontWeight is numeric on the JS surface, so it resets like the numbers.
  pub fn set_font_weight(&mut self, weight: Option<FontWeight>) -> Damage {
    self.font_weight = weight.unwrap_or(DEFAULT_FONT_WEIGHT);
    Damage::Layout
  }
  pub fn set_font_style(&mut self, style: Option<FontStyle>) -> Damage {
    self.font_style = style.unwrap_or(FontStyle::Normal);
    Damage::Layout
  }
  pub fn set_font_stretch(&mut self, v: Option<f32>) -> Damage {
    self.font_stretch = v.unwrap_or(DEFAULT_FONT_STRETCH);
    Damage::Layout
  }
  pub fn set_letter_spacing(&mut self, v: Option<f32>) -> Damage {
    self.letter_spacing = v.unwrap_or(0.0);
    Damage::Layout
  }
  pub fn set_text_alignment(&mut self, alignment: Option<TextAlignment>) -> Damage {
    self.text_alignment = alignment;
    Damage::Layout
  }
  // Detached-only, so paint damage is enough; the shaping key carries it.
  pub fn set_anchor(&mut self, anchor: Option<TextAnchor>) -> Damage {
    self.anchor = anchor;
    Damage::Paint
  }

  pub fn initial_style() -> Style {
    Style { display: Display::Block, ..Default::default() }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Text(self), Self::initial_style())
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Text(self))
  }
}
