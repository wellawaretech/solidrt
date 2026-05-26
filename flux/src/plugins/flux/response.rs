use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::{Class, Ctx, JsLifetime, Object, TypedArray, Value};

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Response")]
pub struct Response {
  #[qjs(skip_trace)]
  pub body: Vec<u8>,
  pub status: u16,
  #[qjs(skip_trace)]
  pub status_text: String,
  #[qjs(skip_trace)]
  pub headers: Vec<(String, String)>,
}

#[rquickjs::methods]
impl Response {
  #[qjs(constructor)]
  pub fn new<'js>(
    body: Opt<Value<'js>>,
    init: Opt<Object<'js>>,
  ) -> rquickjs::Result<Self> {
    let body = match body.0 {
      Some(v) => extract_body(&v)?,
      None => Vec::new(),
    };
    let (status, status_text, headers) = parse_init(init.0.as_ref())?;
    Ok(Response {
      body,
      status,
      status_text,
      headers,
    })
  }

  #[qjs(static)]
  pub fn json<'js>(
    ctx: Ctx<'js>,
    val: Value<'js>,
    init: Opt<Object<'js>>,
  ) -> rquickjs::Result<Self> {
    let json = ctx
      .json_stringify(val)?
      .map(|s| s.to_string())
      .transpose()?
      .unwrap_or_else(|| "null".to_string());
    let (status, status_text, mut headers) = parse_init(init.0.as_ref())?;
    let has_ct = headers
      .iter()
      .any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    if !has_ct {
      headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    Ok(Response {
      body: json.into_bytes(),
      status,
      status_text,
      headers,
    })
  }

  #[qjs(get)]
  pub fn status(&self) -> u16 {
    self.status
  }

  #[qjs(get, rename = "statusText")]
  pub fn status_text(&self) -> String {
    self.status_text.clone()
  }
}

fn extract_body<'js>(val: &Value<'js>) -> rquickjs::Result<Vec<u8>> {
  if val.is_null() || val.is_undefined() {
    return Ok(Vec::new());
  }
  if let Some(s) = val.as_string() {
    return Ok(s.to_string()?.into_bytes());
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  Err(rquickjs::Error::new_from_js_message(
    "body",
    "Response",
    "must be string, Uint8Array, null, or undefined",
  ))
}

fn parse_init<'js>(
  init: Option<&Object<'js>>,
) -> rquickjs::Result<(u16, String, Vec<(String, String)>)> {
  let status: u16 = init.and_then(|o| o.get("status").ok()).unwrap_or(200);
  let status_text: String = init
    .and_then(|o| o.get("statusText").ok())
    .unwrap_or_default();
  let headers = match init.and_then(|o| o.get::<_, Object>("headers").ok()) {
    Some(h) => {
      let mut out = Vec::new();
      for key in h.keys::<String>().flatten() {
        if let Ok(Some(val)) = h.get::<_, Option<String>>(&key) {
          out.push((key, val));
        }
      }
      out
    }
    None => Vec::new(),
  };
  Ok((status, status_text, headers))
}

pub(crate) fn init_response(ctx: &Ctx<'_>) {
  Class::<Response>::define(&ctx.globals()).expect("define Response class");
}