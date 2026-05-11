use crate::rendertree::{Element, ElementKind};

#[derive(Clone, Debug, Default)]
pub struct Span {
  pub text: String,
}

impl Span {
  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Span(self))
  }
}
