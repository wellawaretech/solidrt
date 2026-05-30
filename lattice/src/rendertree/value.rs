// Engine-independent property value. The FFI/plugin layer converts its host
// representation (e.g. a JavaScript value) into a PropValue before handing it to
// rendertree setters, so rendertree never references the host value type. This
// keeps rendertree usable from engines that are not JavaScript.

#[derive(Clone, Debug)]
pub enum PropValue {
  Null,
  Bool(bool),
  Number(f64),
  Text(String),
  List(Vec<PropValue>),
}

impl PropValue {
  // Null also represents a host "undefined".
  pub fn is_null(&self) -> bool {
    matches!(self, PropValue::Null)
  }

  pub fn as_bool(&self) -> Option<bool> {
    match self {
      PropValue::Bool(b) => Some(*b),
      _ => None,
    }
  }

  pub fn as_f64(&self) -> Option<f64> {
    match self {
      PropValue::Number(n) => Some(*n),
      _ => None,
    }
  }

  pub fn as_str(&self) -> Option<&str> {
    match self {
      PropValue::Text(s) => Some(s),
      _ => None,
    }
  }

  pub fn as_list(&self) -> Option<&[PropValue]> {
    match self {
      PropValue::List(items) => Some(items),
      _ => None,
    }
  }
}