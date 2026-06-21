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
  // A keyed object, insertion-ordered. Lets structured prop values (e.g. a
  // gradient) cross the boundary self-describing, so the adapter decodes them by
  // key rather than by array position.
  Map(Vec<(String, PropValue)>),
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

  pub fn as_map(&self) -> Option<&[(String, PropValue)]> {
    match self {
      PropValue::Map(entries) => Some(entries),
      _ => None,
    }
  }

  // Look up a key in a Map; None for a missing key or any non-Map value.
  pub fn get(&self, key: &str) -> Option<&PropValue> {
    match self {
      PropValue::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
      _ => None,
    }
  }
}
