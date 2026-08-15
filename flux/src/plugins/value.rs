//! `forge::Value` <-> JS marshalling, the one place a neutral value meets the
//! engine.
//!
//! Result types in forge describe themselves as a `Value` (`impl From<T> for
//! Value`); a plugin returns `Neutral(result.into())` and this module encodes it.
//! Isolate ports and other value-carrying surfaces decode with the same rules.
//!
//! The contract, deliberately narrower than structured clone (SolidRT lens:
//! standard vocabulary, simplified semantics):
//!
//! JS -> Value
//! - `null` and `undefined` -> `Null`. Array holes and `undefined` property
//!   values become `Null` too (structured clone keeps `undefined`; JSON drops
//!   the key; one rule is simpler than either).
//! - `number` -> `Int` when integral, finite, and within the safe-integer
//!   range (excluding `-0`), else `Float`. So `1` and `1.0` both cross as
//!   `Int(1)`; a receiver that cares about the distinction is not JS.
//! - `boolean`, `string` -> `Bool`, `String`.
//! - `ArrayBuffer` and any typed-array view -> `Bytes` (a copy of the viewed
//!   range). `DataView` and `Uint8ClampedArray` are not accepted.
//! - Arrays -> `List`; plain objects (`Object.prototype` or null prototype)
//!   -> `Map` of own enumerable string keys, in order.
//! - Everything else throws a `TypeError`: functions, symbols, BigInt, class
//!   instances, `Date`/`Map`/`Set`/`RegExp`, promises. Cyclic or deeper than
//!   `MAX_DEPTH` structures throw as well; there is no visited set, so a cycle
//!   is reported as excessive depth.
//!
//! Value -> JS
//! - `Null` -> `null`; `Int` and `Float` -> `number`; `Bytes` -> `Uint8Array`;
//!   `List` -> array; `Map` -> plain object in pair order.

use rquickjs::{Array, Ctx, Exception, FromJs, IntoJs, Object, TypedArray, Value as JsValue};

use forge::Value;

/// Nesting limit for JS -> Value; deeper (or cyclic) input throws.
const MAX_DEPTH: usize = 256;

/// Largest integer JS represents exactly (`Number.MAX_SAFE_INTEGER`).
const MAX_SAFE: f64 = 9007199254740991.0;

/// Marshalling newtype: `Neutral(value)` as a binding return type encodes the
/// value; as a binding parameter it decodes the argument.
pub struct Neutral(pub Value);

impl<'js> IntoJs<'js> for Neutral {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<JsValue<'js>> {
    into_js(ctx, self.0)
  }
}

impl<'js> FromJs<'js> for Neutral {
  fn from_js(ctx: &Ctx<'js>, value: JsValue<'js>) -> rquickjs::Result<Self> {
    from_js(ctx, value).map(Neutral)
  }
}

pub fn into_js<'js>(ctx: &Ctx<'js>, value: Value) -> rquickjs::Result<JsValue<'js>> {
  match value {
    Value::Null => Ok(JsValue::new_null(ctx.clone())),
    Value::Bool(b) => b.into_js(ctx),
    Value::Int(i) => i.into_js(ctx),
    Value::Float(f) => f.into_js(ctx),
    Value::String(s) => s.into_js(ctx),
    Value::Bytes(b) => bytes_into_js(ctx, b),
    Value::List(items) => {
      let arr = Array::new(ctx.clone())?;
      for (i, item) in items.into_iter().enumerate() {
        arr.set(i, into_js(ctx, item)?)?;
      }
      Ok(arr.into_value())
    }
    Value::Map(pairs) => {
      let obj = Object::new(ctx.clone())?;
      for (k, v) in pairs {
        obj.set(k, into_js(ctx, v)?)?;
      }
      Ok(obj.into_value())
    }
  }
}

/// Owned bytes as a `Uint8Array`. The shared spelling for every binding that
/// hands a buffer to JS.
pub fn bytes_into_js<'js>(ctx: &Ctx<'js>, bytes: Vec<u8>) -> rquickjs::Result<JsValue<'js>> {
  TypedArray::<u8>::new(ctx.clone(), bytes).map(|ta| ta.into_value())
}

pub fn from_js<'js>(ctx: &Ctx<'js>, value: JsValue<'js>) -> rquickjs::Result<Value> {
  decode(ctx, value, 0)
}

fn decode<'js>(ctx: &Ctx<'js>, value: JsValue<'js>, depth: usize) -> rquickjs::Result<Value> {
  if depth > MAX_DEPTH {
    return Err(Exception::throw_type(ctx, "value is too deeply nested or cyclic"));
  }
  if value.is_null() || value.is_undefined() {
    return Ok(Value::Null);
  }
  if let Some(b) = value.as_bool() {
    return Ok(Value::Bool(b));
  }
  if let Some(i) = value.as_int() {
    return Ok(Value::Int(i as i64));
  }
  if let Some(f) = value.as_float() {
    return Ok(number(f));
  }
  if let Some(s) = value.as_string() {
    return Ok(Value::String(s.to_string()?));
  }
  let Some(obj) = value.as_object() else {
    return Err(unsupported(ctx, &value));
  };
  if value.is_function() {
    return Err(unsupported(ctx, &value));
  }
  if let Some(bytes) = view_bytes(obj) {
    return Ok(Value::Bytes(bytes));
  }
  if let Some(arr) = value.as_array() {
    let mut items = Vec::with_capacity(arr.len());
    for item in arr.iter::<JsValue>() {
      items.push(decode(ctx, item?, depth + 1)?);
    }
    return Ok(Value::List(items));
  }
  if !is_plain_object(ctx, obj)? {
    return Err(unsupported(ctx, &value));
  }
  let mut pairs = Vec::new();
  for prop in obj.props::<String, JsValue>() {
    let (k, v) = prop?;
    pairs.push((k, decode(ctx, v, depth + 1)?));
  }
  Ok(Value::Map(pairs))
}

fn number(f: f64) -> Value {
  let integral = f.fract() == 0.0 && f.abs() <= MAX_SAFE && !(f == 0.0 && f.is_sign_negative());
  if integral {
    Value::Int(f as i64)
  } else {
    Value::Float(f)
  }
}

/// The bytes an `ArrayBuffer` or typed-array view covers, or `None` if `obj` is
/// neither. A detached buffer reads as empty.
fn view_bytes(obj: &Object<'_>) -> Option<Vec<u8>> {
  if let Some(ab) = obj.as_array_buffer() {
    return Some(ab.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  macro_rules! try_view {
    ($($t:ty),*) => {
      $(
        if let Some(ta) = obj.as_typed_array::<$t>() {
          return Some(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
        }
      )*
    };
  }
  try_view!(u8, i8, u16, i16, u32, i32, f32, f64, u64, i64);
  None
}

/// `Object.prototype` or a null prototype: a data object, not an instance.
fn is_plain_object<'js>(ctx: &Ctx<'js>, obj: &Object<'js>) -> rquickjs::Result<bool> {
  let Some(proto) = obj.get_prototype() else {
    return Ok(true);
  };
  let object_prototype = Object::new(ctx.clone())?.get_prototype();
  Ok(object_prototype.is_some_and(|p| p.as_value() == proto.as_value()))
}

fn unsupported(ctx: &Ctx<'_>, value: &JsValue<'_>) -> rquickjs::Error {
  Exception::throw_type(ctx, &format!("{} value cannot be sent", value.type_name()))
}
