use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::{Class, Ctx, JsLifetime, Value};
use std::cell::RefCell;

/// A subset of the WHATWG Headers API. Stores entries case-insensitively
/// (header names lowercased on storage) while preserving the value as given.
#[derive(JsLifetime)]
#[rquickjs::class(rename = "Headers")]
pub struct Headers {
  #[qjs(skip_trace)]
  pub(crate) entries: RefCell<Vec<(String, String)>>,
}

impl<'js> Trace<'js> for Headers {
  fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

#[rquickjs::methods]
impl Headers {
  #[qjs(constructor)]
  pub fn new<'js>(init: Opt<Value<'js>>) -> rquickjs::Result<Self> {
    let entries = match init.0 {
      Some(v) => entries_from_init(&v)?,
      None => Vec::new(),
    };
    Ok(Headers {
      entries: RefCell::new(entries),
    })
  }

  pub fn get(&self, name: String) -> Option<String> {
    let name = name.to_ascii_lowercase();
    let entries = self.entries.borrow();
    let mut found: Vec<&str> = entries
      .iter()
      .filter(|(k, _)| k == &name)
      .map(|(_, v)| v.as_str())
      .collect();
    if found.is_empty() {
      None
    } else if found.len() == 1 {
      Some(found.remove(0).to_string())
    } else {
      Some(found.join(", "))
    }
  }

  pub fn set(&self, name: String, value: String) {
    let name = name.to_ascii_lowercase();
    let mut entries = self.entries.borrow_mut();
    entries.retain(|(k, _)| k != &name);
    entries.push((name, value));
  }

  pub fn has(&self, name: String) -> bool {
    let name = name.to_ascii_lowercase();
    self.entries.borrow().iter().any(|(k, _)| k == &name)
  }

  pub fn delete(&self, name: String) {
    let name = name.to_ascii_lowercase();
    self.entries.borrow_mut().retain(|(k, _)| k != &name);
  }

  pub fn append(&self, name: String, value: String) {
    self
      .entries
      .borrow_mut()
      .push((name.to_ascii_lowercase(), value));
  }
}

fn entries_from_init<'js>(val: &Value<'js>) -> rquickjs::Result<Vec<(String, String)>> {
  if val.is_null() || val.is_undefined() {
    return Ok(Vec::new());
  }
  // Headers instance: copy entries.
  if let Ok(other) = Class::<Headers>::from_value(val) {
    let other = other.borrow();
    return Ok(other.entries.borrow().clone());
  }
  // Plain object: iterate own keys.
  if let Some(obj) = val.as_object() {
    let mut out = Vec::new();
    for key in obj.keys::<String>().flatten() {
      if let Ok(Some(v)) = obj.get::<_, Option<String>>(&key) {
        out.push((key.to_ascii_lowercase(), v));
      }
    }
    return Ok(out);
  }
  Err(rquickjs::Error::new_from_js_message(
    "init",
    "Headers",
    "must be a plain object, a Headers instance, null, or undefined",
  ))
}

impl Headers {
  /// Borrow internal entries (lowercased name + raw value) for Rust-side use.
  pub(crate) fn entries(&self) -> Vec<(String, String)> {
    self.entries.borrow().clone()
  }
}

/// Build a Headers instance from a list of (name, value) pairs (Rust side).
pub(crate) fn headers_from_pairs<'js>(
  ctx: &Ctx<'js>,
  pairs: Vec<(String, String)>,
) -> rquickjs::Result<Class<'js, Headers>> {
  let lowered = pairs
    .into_iter()
    .map(|(k, v)| (k.to_ascii_lowercase(), v))
    .collect();
  Class::instance(
    ctx.clone(),
    Headers {
      entries: RefCell::new(lowered),
    },
  )
}

/// Build a Headers instance from a JS init value (plain object, Headers, null/undef).
pub(crate) fn headers_from_init<'js>(
  ctx: &Ctx<'js>,
  init: Option<&Value<'js>>,
) -> rquickjs::Result<Class<'js, Headers>> {
  let entries = match init {
    Some(v) => entries_from_init(v)?,
    None => Vec::new(),
  };
  Class::instance(
    ctx.clone(),
    Headers {
      entries: RefCell::new(entries),
    },
  )
}

pub(crate) fn init_headers(ctx: &Ctx<'_>) {
  Class::<Headers>::define(&ctx.globals()).expect("define Headers class");
}