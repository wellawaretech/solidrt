// The paragraph engine: the whole text handed to one Impeller paragraph per
// width. Kept behind `Text::paragraph_engine` as a reference and fallback;
// spans' hit testing, atoms, floats, indent and wrap do nothing here.
use super::shape::ParaKey;
use super::{Text, TextOverflow, MAX_CACHED_WIDTHS};
use crate::impellers::{DisplayListBuilder, Paragraph, ParagraphBuilder, Point, Size, TypographyContext};
use crate::rendertree::MeasureContext;
use taffy::AvailableSpace;

#[derive(Clone, Default)]
pub(super) struct ParaCache {
  key: Option<ParaKey>,
  entries: Vec<(f32, Paragraph)>,
}

// Manual impl: impellers::Paragraph has no Debug.
impl std::fmt::Debug for ParaCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "ParaCache({} entries)", self.entries.len())
  }
}

impl Text {
  pub(super) fn build_paragraph(
    &self,
    typography: &TypographyContext,
    builder: &mut DisplayListBuilder,
    origin: Point,
    width: f32,
  ) {
    if let Some(paragraph) = self.shaped(typography, width) {
      builder.draw_paragraph(&paragraph, origin);
    }
  }

  pub(super) fn measure_paragraph(&self, ctx: &MeasureContext) -> Size {

    let Some(intrinsic) = self.shaped(&ctx.platform.typography(), f32::MAX) else {
      return Size::zero();
    };

    let max_intrinsic_width = intrinsic.get_max_intrinsic_width();
    let min_intrinsic_width = intrinsic.get_min_intrinsic_width();

    let width = ctx.known.width.unwrap_or_else(|| match ctx.available.width {
      AvailableSpace::Definite(w) => max_intrinsic_width.min(w),
      AvailableSpace::MaxContent => max_intrinsic_width,
      AvailableSpace::MinContent => min_intrinsic_width,
    });

    let Some(paragraph) = self.shaped(&ctx.platform.typography(), width) else {
      return Size::zero();
    };

    let height = ctx.known.height.unwrap_or_else(|| paragraph.get_height());

    Size::new(width, height)
  }

  // Shape (or fetch the cached) paragraph for `width`. One paragraph serves
  // measure and paint: foreground and alignment are baked in even where
  // measurement does not need them, since they don't change the metrics.
  fn shaped(&self, typography: &TypographyContext, width: f32) -> Option<Paragraph> {
    let mut cache = self.cache.borrow_mut();
    if !cache.key.as_ref().is_some_and(|k| k.matches(self)) {
      cache.entries.clear();
      cache.key = Some(ParaKey::of(self));
    }
    if let Some((_, paragraph)) = cache.entries.iter().find(|(w, _)| *w == width) {
      return Some(paragraph.clone());
    }
    crate::rendertree::counters::note_para_shape();

    // Paragraph-level settings are read from the first pushed style, so every
    // run's style carries them; inner runs' copies are ignored by Impeller.
    let mut para_builder = ParagraphBuilder::new(typography)?;
    let mut pushed = 0;
    // Atoms are an owned-path feature; the paragraph engine has no
    // placeholders, so they are left out here.
    for run in self.runs.iter().filter(|r| r.atom.is_none()) {
      let mut style = self.paragraph_style(&run.overrides.resolve(self));
      style.set_text_alignment(self.text_alignment);
      // 0 means no cap: keep txt's unlimited default. Passing 0 through reads
      // as "the first line is the last" in Skia's line breaker, so every
      // paragraph would shape single-line.
      if self.max_lines > 0 {
        style.set_max_lines(self.max_lines);
      }
      if let TextOverflow::Ellipsis(s) = &self.text_overflow {
        style.set_ellipsis(Some(s));
      }
      para_builder.push_style(&style);
      para_builder.add_text(&run.text);
      pushed += 1;
    }
    if pushed == 0 {
      // Empty text still needs a style for its (zero-line) metrics.
      para_builder.push_style(&self.paragraph_style(&self.run_style()));
    }
    let paragraph = para_builder.build(width)?;

    if cache.entries.len() >= MAX_CACHED_WIDTHS {
      cache.entries.remove(0);
    }
    cache.entries.push((width, paragraph.clone()));
    Some(paragraph)
  }
}
