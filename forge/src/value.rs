//! Engine-free neutral value.
//!
//! The vocabulary that crosses between forge and any scripting engine (and,
//! later, between isolates over ports): the structured-clone / CBOR / msgpack
//! set with integers kept distinct from floats. forge result types describe
//! themselves as a `Value` (`impl From<T> for Value`), so the host's marshalling
//! layer converts `Value` <-> engine value in exactly one place instead of once
//! per result type. Maps are ordered pairs so insertion order survives a round
//! trip; lookups are linear, which is fine for message-sized data.

/// A neutral value. `Int` covers what a JS `number` holds exactly (and what
/// SQLite / JSON distinguish); the host maps both `Int` and `Float` to its
/// number type. `Bytes` is an owned byte buffer, never a view.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
  Null,
  Bool(bool),
  Int(i64),
  Float(f64),
  String(String),
  Bytes(Vec<u8>),
  List(Vec<Value>),
  Map(Vec<(String, Value)>),
}

impl Value {
  /// A `List` from anything that yields values.
  pub fn list<I, T>(items: I) -> Value
  where
    I: IntoIterator<Item = T>,
    T: Into<Value>,
  {
    Value::List(items.into_iter().map(Into::into).collect())
  }

  /// A `Map` from anything that yields `(key, value)` pairs, in order.
  pub fn map<I, K, T>(pairs: I) -> Value
  where
    I: IntoIterator<Item = (K, T)>,
    K: Into<String>,
    T: Into<Value>,
  {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect())
  }
}

impl From<bool> for Value {
  fn from(v: bool) -> Value {
    Value::Bool(v)
  }
}

impl From<i32> for Value {
  fn from(v: i32) -> Value {
    Value::Int(v as i64)
  }
}

impl From<i64> for Value {
  fn from(v: i64) -> Value {
    Value::Int(v)
  }
}

impl From<u16> for Value {
  fn from(v: u16) -> Value {
    Value::Int(v as i64)
  }
}

impl From<u32> for Value {
  fn from(v: u32) -> Value {
    Value::Int(v as i64)
  }
}

/// `u64` beyond `i64::MAX` degrades to `Float` rather than failing; sizes and
/// counts never get there in practice.
impl From<u64> for Value {
  fn from(v: u64) -> Value {
    i64::try_from(v).map(Value::Int).unwrap_or(Value::Float(v as f64))
  }
}

impl From<f64> for Value {
  fn from(v: f64) -> Value {
    Value::Float(v)
  }
}

impl From<String> for Value {
  fn from(v: String) -> Value {
    Value::String(v)
  }
}

impl From<&str> for Value {
  fn from(v: &str) -> Value {
    Value::String(v.to_string())
  }
}

impl From<Vec<u8>> for Value {
  fn from(v: Vec<u8>) -> Value {
    Value::Bytes(v)
  }
}

/// `None` is `Null`. Where a host distinguishes "absent" from `null` (JS
/// `undefined`, an omitted map key), the record's own `From` impl decides.
impl<T: Into<Value>> From<Option<T>> for Value {
  fn from(v: Option<T>) -> Value {
    v.map(Into::into).unwrap_or(Value::Null)
  }
}
