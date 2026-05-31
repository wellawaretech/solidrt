use crate::rendertree::{BuildContext, Buildable, Element, ElementKind};
use alloy::impellers::DisplayListBuilder;
use alloy::AlloyCommand;
use std::sync::mpsc::Sender;
use taffy::{prelude::percent, Display, FlexDirection, Size, Style};

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
  // Title and fullscreen are not plain fields: changing them must push a command
  // to the windowing backend. That behavior lives here, in the element, so the
  // binding layer only has to decode the value and call the setter.
  // Return whether the change affects layout (never, for window chrome). Both
  // also push a command to the windowing backend.
  pub fn set_title(&mut self, title: String, cmd_tx: &Sender<AlloyCommand>) -> bool {
    self.title = title;
    cmd_tx.send(AlloyCommand::SetTitle(self.title.clone())).ok();
    false
  }

  pub fn set_fullscreen(&mut self, fullscreen: bool, cmd_tx: &Sender<AlloyCommand>) -> bool {
    self.fullscreen = fullscreen;
    cmd_tx.send(AlloyCommand::SetFullscreen(fullscreen)).ok();
    false
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