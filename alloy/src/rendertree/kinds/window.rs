use crate::gpu::WindowShader;
use crate::impellers::DisplayListBuilder;
use crate::rendertree::Damage;
use crate::rendertree::{BuildContext, Buildable, Element, ElementKind};
use crate::AlloyCommand;
use std::cell::RefCell;
use std::sync::mpsc::Sender;
use taffy::{prelude::percent, Display, FlexDirection, Size, Style};

#[derive(Clone, Debug)]
pub struct Window {
  pub title: String,
  pub fullscreen: bool,
  /// The declared window shader (see `Context::set_window_shader`); None
  /// draws the frame unshaded.
  pub shader: Option<WindowShader>,
  // A shader change recorded since the last build, pushed to the raster
  // thread the next time this element builds (see set_shader). The outer
  // Option is "changed at all", the inner the new declaration.
  pending_shader: RefCell<Option<Option<WindowShader>>>,
}

impl Default for Window {
  fn default() -> Self {
    Window { title: "SolidRT".to_string(), fullscreen: false, shader: None, pending_shader: RefCell::new(None) }
  }
}

impl Buildable for Window {
  fn build<'a>(&'a self, ctx: &mut BuildContext<'a>, _builder: &mut DisplayListBuilder) {
    if let Some(change) = self.pending_shader.borrow_mut().take() {
      if let Err(e) = ctx.alloy.set_window_shader(change) {
        log::warn!("[window] shader: {e}");
      }
    }
  }
}

impl Window {
  // Title and fullscreen are not plain fields: changing them must push a command
  // to the windowing backend. That behavior lives here, in the element, so the
  // binding layer only has to decode the value and call the setter. Title is
  // pure chrome (Damage::None); fullscreen changes the window size (Layout).
  pub fn set_title(&mut self, title: String, cmd_tx: &Sender<AlloyCommand>) -> Damage {
    self.title = title;
    cmd_tx.send(AlloyCommand::SetTitle(self.title.clone())).ok();
    Damage::None
  }

  pub fn set_fullscreen(&mut self, fullscreen: bool, cmd_tx: &Sender<AlloyCommand>) -> Damage {
    self.fullscreen = fullscreen;
    cmd_tx.send(AlloyCommand::SetFullscreen(fullscreen)).ok();
    Damage::Layout
  }

  /// Declare or clear the window shader. Like texture params, the write only
  /// records the change; it is pushed to the raster thread at the next frame
  /// (`build` on the rebuild path, `take_pending_shader` on the present-only
  /// reuse path), so reactive updates stay paced to real frames and the
  /// command is ordered ahead of the frame that should show it. Present, not
  /// Paint: the tree's content and its built display list stay valid - the
  /// shader draws over the finished frame.
  pub fn set_shader(&mut self, shader: Option<WindowShader>) -> Damage {
    self.shader = shader.clone();
    *self.pending_shader.get_mut() = Some(shader);
    Damage::Present
  }

  /// Take the recorded shader change without a build (the present-only reuse
  /// path flushes it before resubmitting the cached display list).
  pub fn take_pending_shader(&mut self) -> Option<Option<WindowShader>> {
    self.pending_shader.get_mut().take()
  }

  pub fn initial_style() -> Style {
    Style {
      display: Display::Flex,
      flex_direction: FlexDirection::Column,
      size: Size { width: percent(1.0), height: percent(1.0) },
      ..Default::default()
    }
  }

  pub fn with_layout(self) -> Element {
    Element::with_layout(ElementKind::Window(self), Self::initial_style())
  }
}
