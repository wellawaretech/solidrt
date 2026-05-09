use crate::rendertree::{BuildContext, Buildable, Element, ElementKind};
use alloy::impellers::DisplayListBuilder;
use taffy::{Display, FlexDirection, Size, Style, prelude::length};

#[derive(Clone, Debug)]
pub struct Window {
    pub title: String,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            title: "Solid-RT".to_string(),
        }
    }
}

impl Buildable for Window {
    fn build<'a>(&'a self, _ctx: &mut BuildContext<'a>, _builder: &mut DisplayListBuilder) {}
}

impl Window {
    pub fn with_layout(self) -> Element {
        Element::with_layout(
            ElementKind::Window(self),
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size { width: length(800.0), height: length(600.0) },
                ..Default::default()
            },
        )
    }
}
