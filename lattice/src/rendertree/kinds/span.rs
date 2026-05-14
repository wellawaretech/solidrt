use crate::rendertree::{Element, ElementKind};
use rquickjs::Value;

#[derive(Clone, Debug, Default)]
pub struct Span {
  pub text: String,
}

impl Span {
  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "text" => { self.text = value.get::<String>().expect("text must be a string"); Some(true) }
      _ => None,
    }
  }

  pub fn no_layout(self) -> Element {
    Element::no_layout(ElementKind::Span(self))
  }
}
