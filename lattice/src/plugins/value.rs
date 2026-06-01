// The binding-layer boundary value. The FFI marshals a host value (e.g. a
// JavaScript value via QuickJS) into a PropValue, and the JSX property adapter
// decodes it into the native Rust values that rendertree setters take. This
// type lives in the plugin layer on purpose: rendertree never sees it.

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
