use crate::rendertree::{BuildContext, Buildable, Element, ElementKind, PropValue};
use alloy::impellers::DisplayListBuilder;
use alloy::AlloyCommand;
use std::sync::mpsc::Sender;
use taffy::{prelude::{length, percent}, Display, FlexDirection, Size, Style};

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
  pub fn set_property(
    &mut self,
    property: &str,
    value: &PropValue,
    cmd_tx: &Sender<AlloyCommand>,
  ) -> Option<bool> {
    match property {
      "title" => {
        self.title = value.as_str().expect("title must be a string").to_string();
        cmd_tx.send(AlloyCommand::SetTitle(self.title.clone())).ok();
        Some(false)
      }
      "fullscreen" => {
        self.fullscreen = value.as_bool().expect("fullscreen must be a boolean");
        cmd_tx.send(AlloyCommand::SetFullscreen(self.fullscreen)).ok();
        Some(false)
      }
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
          width: percent(1.0),
          height: percent(1.0),
        },
        ..Default::default()
      },
    )
  }
}
