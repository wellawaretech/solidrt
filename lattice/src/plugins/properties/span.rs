use super::str_of;
use crate::plugins::value::PropValue;
use crate::rendertree::Span;

pub fn apply(span: &mut Span, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "text" => span.set_text(str_of(value, "text").to_string()),
    _ => return None,
  })
}