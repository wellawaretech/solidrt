use crate::rendertree::{Element, ElementKind, PropValue};

#[derive(Clone, Debug, Default)]
pub struct Span {
  pub text: String,
}

impl Span {
  pub fn set_property(&mut self, property: &str, value: &PropValue) -> Option<bool> {
    match property {
      "text" => { self.text = value.as_str().expect("text must be a string").to_string(); Some(true) }
      _ => None,
    }
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Span(self))
  }
}
