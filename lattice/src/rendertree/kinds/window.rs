use crate::rendertree::{BuildContext, Buildable, Element, ElementKind};
use alloy::impellers::DisplayListBuilder;
use rquickjs::Value;
use taffy::{prelude::length, Display, FlexDirection, Size, Style};

#[derive(Clone, Debug)]
pub struct Window {
  pub title: String,
  pub fullscreen: bool,
}

impl Default for Window {
  fn default() -> Self {
    Window {
      title: "SolidRT".to_string(),
      fullscreen: false,
    }
  }
}

impl Buildable for Window {
  fn build<'a>(&'a self, _ctx: &mut BuildContext<'a>, _builder: &mut DisplayListBuilder) {}
}

impl Window {
  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "title" => { self.title = value.get::<String>().expect("title must be a string"); Some(false) }
      "fullscreen" => { self.fullscreen = value.get::<bool>().expect("fullscreen must be a boolean"); Some(false) }
      _ => None,
    }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(
      ElementKind::Window(self),
      Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        size: Size {
          width: length(800.0),
          height: length(600.0),
        },
        ..Default::default()
      },
    )
  }
}
