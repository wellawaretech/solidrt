use crate::rendertree::{BuildContext, Buildable, Measurable, Node, Primitive};
use super::PaintState;
use alloy::impellers::{
    DisplayListBuilder, FontStyle, ParagraphBuilder, ParagraphStyle, Point, TextAlignment,
    TypographyContext,
};
use taffy::prelude::*;

#[derive(Clone, Debug)]
pub struct TextNode {
    pub text: String,
    pub font_size: f32,
    pub font_style: FontStyle,
    pub text_alignment: TextAlignment,
    pub max_lines: u32,
    pub paint: PaintState,
}

impl Default for TextNode {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_size: 20.0,
            font_style: FontStyle::Normal,
            text_alignment: TextAlignment::Left,
            max_lines: 0,
            paint: PaintState::default(),
        }
    }
}

impl Buildable for TextNode {
    fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, builder: &mut DisplayListBuilder) {
        let mut style = ParagraphStyle::default();
        let paint = self.paint.to_paint();
        style.set_foreground(&paint);
        style.set_font_size(self.font_size);
        style.set_font_style(self.font_style);
        style.set_text_alignment(self.text_alignment);
        style.set_max_lines(self.max_lines);

        let Some(mut para_builder) = ParagraphBuilder::new(ctx.typography_ctx) else {
            return;
        };
        para_builder.push_style(&style);
        para_builder.add_text(&self.text);

        let Some(paragraph) = para_builder.build(ctx.size.w) else {
            return;
        };
        builder.draw_paragraph(&paragraph, Point::new(0.0, 0.0));
    }
}

impl Measurable for TextNode {
    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        typography_ctx: &TypographyContext,
    ) -> Size<f32> {
        if let (Some(w), Some(h)) = (known_dimensions.width, known_dimensions.height) {
            return Size { width: w, height: h };
        }

        let Some(mut para_builder) = ParagraphBuilder::new(typography_ctx) else {
            return Size::ZERO;
        };

        let mut style = ParagraphStyle::default();
        style.set_font_size(self.font_size);
        style.set_font_style(self.font_style);

        para_builder.push_style(&style);
        para_builder.add_text(&self.text);

        let Some(paragraph) = para_builder.build(f32::MAX) else {
            return Size::ZERO;
        };

        let max_intrinsic_width = paragraph.get_max_intrinsic_width();
        let min_intrinsic_width = paragraph.get_min_intrinsic_width();

        let width = known_dimensions
            .width
            .unwrap_or_else(|| match available_space.width {
                AvailableSpace::Definite(w) => max_intrinsic_width.min(w),
                AvailableSpace::MaxContent => max_intrinsic_width,
                AvailableSpace::MinContent => min_intrinsic_width,
            });

        let Some(mut para_builder) = ParagraphBuilder::new(typography_ctx) else {
            return Size::ZERO;
        };
        para_builder.push_style(&style);
        para_builder.add_text(&self.text);

        let Some(paragraph) = para_builder.build(width) else {
            return Size::ZERO;
        };

        let height = known_dimensions
            .height
            .unwrap_or_else(|| paragraph.get_height());

        Size { width, height }
    }
}

impl From<TextNode> for Node {
    fn from(text: TextNode) -> Node {
        Node::new(
            Primitive::Text(text),
            Some(Style {
                display: Display::Block,
                ..Default::default()
            }),
        )
    }
}