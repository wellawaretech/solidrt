use rquickjs::{
  function::MutFn, promise::Promised, Ctx, Function, IntoJs, Object, TypedArray, Value,
};
use std::io;
use std::rc::Rc;

use crate::pending::PendingOps;
use crate::plugins::body::attach_body;
use crate::plugins::http::{reqwest_err, HttpClient};

struct ResponseData {
  status: u16,
  status_text: String,
  url: String,
  ok: bool,
  headers_json: String,
  body: Vec<u8>,
}

fn build_response<'js>(ctx: &Ctx<'js>, data: ResponseData) -> rquickjs::Result<Value<'js>> {
  let body = Rc::new(data.body);
  let headers = ctx.json_parse(data.headers_json)?;

  let obj = Object::new(ctx.clone())?;
  obj.set("ok", data.ok)?;
  obj.set("status", data.status)?;
  obj.set("statusText", data.status_text)?;
  obj.set("url", data.url)?;
  obj.set("headers", headers)?;

  let body_for_closure = body.clone();
  attach_body(
    ctx,
    &obj,
    move || {
      let body = body_for_closure.clone();
      async move { Ok((*body).clone()) }
    },
    true,
  )?;

  Ok(obj.into_value())
}

fn headers_to_json(headers: &reqwest::header::HeaderMap) -> String {
  let mut items = Vec::new();
  for (name, value) in headers.iter() {
    let name = name.as_str();
    let value = value.to_str().unwrap_or("");
    let escaped_name = name.replace('\\', "\\\\").replace('"', "\\\"");
    let escaped_value = value.replace('\\', "\\\\").replace('"', "\\\"");
    items.push(format!(r#""{}":"{}""#, escaped_name, escaped_value));
  }
  format!("{{{}}}", items.join(","))
}

fn status_text(status: reqwest::StatusCode) -> &'static str {
  match status.as_u16() {
    200 => "OK",
    201 => "Created",
    204 => "No Content",
    301 => "Moved Permanently",
    302 => "Found",
    304 => "Not Modified",
    400 => "Bad Request",
    401 => "Unauthorized",
    403 => "Forbidden",
    404 => "Not Found",
    405 => "Method Not Allowed",
    409 => "Conflict",
    429 => "Too Many Requests",
    500 => "Internal Server Error",
    502 => "Bad Gateway",
    503 => "Service Unavailable",
    _ => "",
  }
}

pub(crate) fn init_fetch(ctx: &Ctx<'_>) {
  let globals = ctx.globals();

  let fetch_fn = Function::new(
    ctx.clone(),
    MutFn::from(
      |ctx: Ctx<'_>,
       url: String,
       opts: rquickjs::function::Opt<Object<'_>>|
       -> rquickjs::Result<Promised<_>> {
        let client = ctx
          .userdata::<HttpClient>()
          .expect("http client")
          .0
          .clone();
        let pending = ctx
          .userdata::<PendingOps>()
          .expect("pending ops")
          .clone();

        let method = opts
          .0
          .as_ref()
          .and_then(|o| o.get::<_, Option<String>>("method").ok().flatten())
          .unwrap_or_else(|| "GET".to_string())
          .to_uppercase();

        let body: Option<Vec<u8>> = opts.0.as_ref().and_then(|o| {
          let val: Value = o.get("body").ok()?;
          if val.is_null() || val.is_undefined() {
            return None;
          }
          if let Some(s) = val.as_string() {
            Some(s.to_string().ok()?.into_bytes())
          } else if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
            Some(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
          } else {
            None
          }
        });

        let headers: Vec<(String, String)> = opts
          .0
          .as_ref()
          .map(|o| {
            let h: Object = match o.get("headers") {
              Ok(h) => h,
              Err(_) => return Vec::new(),
            };
            let mut out = Vec::new();
            for key in h.keys::<String>() {
              if let Ok(key) = key {
                if let Ok(Some(val)) = h.get::<_, Option<String>>(&key) {
                  out.push((key, val));
                }
              }
            }
            out
          })
          .unwrap_or_default();

        Ok(Promised(async move {
          pending.hold();
          let r = do_fetch(client, &method, &url, headers, body).await;
          pending.release();
          r
        }))
      },
    ),
  )
  .expect("create fetch function");

  globals.set("fetch", fetch_fn).expect("set fetch global");
}

async fn do_fetch(
  client: Rc<reqwest::Client>,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<Vec<u8>>,
) -> rquickjs::Result<ResponseData> {
  let mut req = match method {
    "GET" => client.get(url),
    "POST" => client.post(url),
    "PUT" => client.put(url),
    "DELETE" => client.delete(url),
    "PATCH" => client.patch(url),
    "HEAD" => client.head(url),
    _ => client.request(
      method.parse().map_err(|_| {
        rquickjs::Error::Io(io::Error::new(
          io::ErrorKind::InvalidInput,
          format!("invalid HTTP method: {}", method),
        ))
      })?,
      url,
    ),
  };

  for (key, val) in &headers {
    req = req.header(key.as_str(), val.as_str());
  }

  if let Some(body) = body {
    req = req.body(body);
  }

  let resp = req.send().await.map_err(reqwest_err)?;
  let status = resp.status();
  let resp_url = resp.url().to_string();
  let resp_headers = headers_to_json(resp.headers());
  let resp_body = resp.bytes().await.map_err(reqwest_err)?;

  Ok(ResponseData {
    status: status.as_u16(),
    status_text: status_text(status).to_string(),
    url: resp_url,
    ok: status.is_success(),
    headers_json: resp_headers,
    body: resp_body.to_vec(),
  })
}

impl<'js> IntoJs<'js> for ResponseData {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    build_response(ctx, self)
  }
}