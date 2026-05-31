use crate::rendertree::{Element, ElementKind};

#[derive(Clone, Debug, Default)]
pub struct Span {
  pub text: String,
}

impl Span {
  // Span text feeds the parent paragraph's measurement, so it affects layout.
  pub fn set_text(&mut self, text: String) -> bool { self.text = text; true }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Span(self))
  }
}
