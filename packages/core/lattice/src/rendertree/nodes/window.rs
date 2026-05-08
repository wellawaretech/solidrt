use crate::rendertree::{BuildContext, Buildable, Node, Primitive};
use alloy::impellers::DisplayListBuilder;
use taffy::{Display, FlexDirection, Style, Size, prelude::length};

#[derive(Clone, Debug)]
pub struct WindowNode {
    pub title: String,
    pub vsync: bool,
    pub fps: bool,
}

impl Default for WindowNode {
    fn default() -> Self {
        WindowNode {
            title: "Solid-RT".to_string(),
            vsync: true,
            fps: false,
        }
    }
}

impl Buildable for WindowNode {
    fn build<'a>(&'a self, _ctx: &mut BuildContext<'a>, _builder: &mut DisplayListBuilder) {}
}

impl From<WindowNode> for Node {
    fn from(window: WindowNode) -> Node {
        Node::new(
            Primitive::Window(window),
            Some(Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size { width: length(800.0), height: length(600.0) },
                ..Default::default()
            }),
        )
    }
}