// Provides HTTP-backed flux:fs and fetch implementations that route through
// the cli's dev server.
//
// File/dir reads use GET on the cli; file.write uses PUT (cli writes the body
// into state.sourceDir). Global fetch is rewritten to call the cli's
// /__proxy__ endpoint with the original URL in the X-SRT-Proxy-Url header;
// the cli forwards the request and relays the response.
//
// File vs directory disambiguation uses the cli's X-SRT-Type response header
// (set by server.ts) to avoid ambiguity around .json files.

use flux::rquickjs::{
  function::{MutFn, Opt},
  module::{Declarations, Exports, ModuleDef},
  promise::Promised,
  Array, Ctx, Function, IntoJs, JsLifetime, Object, TypedArray, Value,
};
use flux::{attach_body, do_fetch, JsResponseData, JsResult};
use std::io;
use std::rc::Rc;

const SRT_TYPE_HEADER: &str = "x-srt-type";

#[derive(Clone, JsLifetime)]
struct ProxyState {
  #[qjs(skip_trace)]
  base: Rc<String>,
  #[qjs(skip_trace)]
  client: Rc<reqwest::Client>,
}

fn http_err(e: impl std::fmt::Display) -> flux::rquickjs::Error {
  flux::rquickjs::Error::Io(io::Error::new(io::ErrorKind::Other, e.to_string()))
}

fn url_for(base: &str, path: &str) -> String {
  let p = path.strip_prefix("./").unwrap_or(path);
  let p = p.strip_prefix('/').unwrap_or(p);
  format!("http://{}/{}", base, p)
}

fn header_str<'a>(resp: &'a reqwest::Response, name: &str) -> Option<&'a str> {
  resp.headers().get(name).and_then(|v| v.to_str().ok())
}

struct ProxyStat {
  size: u64,
}

impl<'js> IntoJs<'js> for ProxyStat {
  fn into_js(self, ctx: &Ctx<'js>) -> flux::rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("size", self.size)?;
    obj.set("type", "file")?;
    // mtime not exposed by cli today; proxy returns 0.
    obj.set("mtime", 0_i64)?;
    Ok(obj.into_value())
  }
}

struct DirEntries(Vec<(String, String)>);

impl<'js> IntoJs<'js> for DirEntries {
  fn into_js(self, ctx: &Ctx<'js>) -> flux::rquickjs::Result<Value<'js>> {
    let arr = Array::new(ctx.clone())?;
    for (i, (name, kind)) in self.0.into_iter().enumerate() {
      let entry = Object::new(ctx.clone())?;
      entry.set("name", name)?;
      entry.set("type", kind)?;
      arr.set(i, entry)?;
    }
    Ok(arr.into_value())
  }
}

fn entry_type_from_num(t: u64) -> String {
  match t {
    1 => "file".to_string(),
    2 => "directory".to_string(),
    _ => "other".to_string(),
  }
}

fn build_proxy_file<'js>(ctx: Ctx<'js>, path: String) -> flux::rquickjs::Result<Object<'js>> {
  let state = ctx.userdata::<ProxyState>().expect("proxy state").clone();
  let url = Rc::new(url_for(&state.base, &path));
  let client = state.client.clone();
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path)?;

  let url_for_body = url.clone();
  let client_for_body = client.clone();
  attach_body(
    &ctx,
    &obj,
    move || {
      let url = url_for_body.clone();
      let client = client_for_body.clone();
      async move {
        let resp = client.get(&*url).send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        if !status.is_success() {
          return Err(format!("HTTP {} for {}", status.as_u16(), &*url));
        }
        resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.to_string())
      }
    },
    false,
  )?;

  let exists_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let url = url.clone();
      let client = client.clone();
      move |_ctx: Ctx<'_>| -> flux::rquickjs::Result<Promised<_>> {
        let url = url.clone();
        let client = client.clone();
        Ok(Promised(async move {
          let ok = match client.head(&*url).send().await {
            Ok(resp) if resp.status().is_success() => header_str(&resp, SRT_TYPE_HEADER) != Some("directory"),
            _ => false,
          };
          Ok::<bool, flux::rquickjs::Error>(ok)
        }))
      }
    }),
  )
  .expect("create file.exists");
  obj.set("exists", exists_fn)?;

  let stat_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let url = url.clone();
      let client = client.clone();
      move |_ctx: Ctx<'_>| -> flux::rquickjs::Result<Promised<_>> {
        let url = url.clone();
        let client = client.clone();
        Ok(Promised(async move {
          let resp = client.head(&*url).send().await.map_err(http_err)?;
          if !resp.status().is_success() {
            return Err(http_err(format!("stat HTTP {} for {}", resp.status().as_u16(), &*url)));
          }
          let size = header_str(&resp, "content-length").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
          Ok::<ProxyStat, flux::rquickjs::Error>(ProxyStat { size })
        }))
      }
    }),
  )
  .expect("create file.stat");
  obj.set("stat", stat_fn)?;

  let write_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let url = url.clone();
      let client = client.clone();
      move |ctx: Ctx<'_>, data: Value<'_>| -> flux::rquickjs::Result<Promised<_>> {
        let bytes = if let Some(s) = data.as_string() {
          s.to_string()?.into_bytes()
        } else if let Ok(ta) = TypedArray::<u8>::from_value(data.clone()) {
          ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default()
        } else {
          return Err(
            ctx.throw(
              flux::rquickjs::String::from_str(ctx.clone(), "write: data must be string or Uint8Array")
                .expect("create error string")
                .into(),
            ),
          );
        };
        let url = url.clone();
        let client = client.clone();
        Ok(Promised(async move {
          let resp = client.put(&*url).body(bytes).send().await.map_err(http_err)?;
          let status = resp.status();
          if !status.is_success() {
            return Err(http_err(format!("write HTTP {} for {}", status.as_u16(), &*url)));
          }
          Ok::<(), flux::rquickjs::Error>(())
        }))
      }
    }),
  )
  .expect("create file.write");
  obj.set("write", write_fn)?;

  Ok(obj)
}

fn build_proxy_dir<'js>(ctx: Ctx<'js>, path: String) -> flux::rquickjs::Result<Object<'js>> {
  let state = ctx.userdata::<ProxyState>().expect("proxy state").clone();
  let url = Rc::new(url_for(&state.base, &path));
  let client = state.client.clone();
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path)?;

  let entries_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let url = url.clone();
      let client = client.clone();
      move |_ctx: Ctx<'_>| -> flux::rquickjs::Result<Promised<_>> {
        let url = url.clone();
        let client = client.clone();
        Ok(Promised(async move {
          let resp = client.get(&*url).send().await.map_err(http_err)?;
          if !resp.status().is_success() {
            return Err(http_err(format!("entries HTTP {} for {}", resp.status().as_u16(), &*url)));
          }
          let body = resp.bytes().await.map_err(http_err)?;
          let arr: Vec<serde_json::Value> =
            serde_json::from_slice(&body).map_err(|e| http_err(format!("invalid dir listing: {e}")))?;
          let items: Vec<(String, String)> = arr
            .into_iter()
            .filter_map(|v| {
              let name = v.get("name")?.as_str()?.to_string();
              let kind = v.get("type")?.as_u64().map(entry_type_from_num)?;
              Some((name, kind))
            })
            .collect();
          Ok::<DirEntries, flux::rquickjs::Error>(DirEntries(items))
        }))
      }
    }),
  )
  .expect("create dir.entries");
  obj.set("entries", entries_fn)?;

  let exists_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let url = url.clone();
      let client = client.clone();
      move |_ctx: Ctx<'_>| -> flux::rquickjs::Result<Promised<_>> {
        let url = url.clone();
        let client = client.clone();
        Ok(Promised(async move {
          let ok = match client.head(&*url).send().await {
            Ok(resp) if resp.status().is_success() => header_str(&resp, SRT_TYPE_HEADER) == Some("directory"),
            _ => false,
          };
          Ok::<bool, flux::rquickjs::Error>(ok)
        }))
      }
    }),
  )
  .expect("create dir.exists");
  obj.set("exists", exists_fn)?;

  Ok(obj)
}

pub struct ProxyFsModule;

impl ModuleDef for ProxyFsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("file")?;
    decl.declare("dir")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    let file_fn = Function::new(ctx.clone(), build_proxy_file).expect("create proxy file function");
    let dir_fn = Function::new(ctx.clone(), build_proxy_dir).expect("create proxy dir function");
    exports.export("file", file_fn)?;
    exports.export("dir", dir_fn)?;
    Ok(())
  }
}

fn extract_fetch_body<'js>(val: &Value<'js>) -> Option<Vec<u8>> {
  if val.is_null() || val.is_undefined() {
    return None;
  }
  if let Some(s) = val.as_string() {
    return Some(s.to_string().ok()?.into_bytes());
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Some(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  None
}

fn extract_fetch_headers<'js>(opts: &Object<'js>) -> Vec<(String, String)> {
  let h: Object = match opts.get("headers") {
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
}

pub fn install_proxy_state(ctx: Ctx<'_>, dev_server: String, http: bool) {
  let client = reqwest::Client::builder().user_agent("lattice-go-proxy").build().expect("build proxy http client");

  let base = Rc::new(dev_server);
  ctx.store_userdata(ProxyState { base: base.clone(), client: Rc::new(client) }).expect("store proxy state");

  if http {
    let proxy_url = Rc::new(format!("http://{}/__proxy__", &*base));
    let fetch_fn = Function::new(
      ctx.clone(),
      MutFn::from({
        let proxy_url = proxy_url.clone();
        move |ctx: Ctx<'_>, url: String, opts: Opt<Object<'_>>| -> flux::rquickjs::Result<Promised<_>> {
          let state = ctx.userdata::<ProxyState>().expect("proxy state").clone();

          let method = opts
            .0
            .as_ref()
            .and_then(|o| o.get::<_, Option<String>>("method").ok().flatten())
            .unwrap_or_else(|| "GET".to_string())
            .to_uppercase();

          let body = opts.0.as_ref().and_then(|o| o.get::<_, Value>("body").ok()).and_then(|v| extract_fetch_body(&v));

          let mut headers = opts.0.as_ref().map(extract_fetch_headers).unwrap_or_default();
          headers.push(("x-srt-proxy-url".to_string(), url));

          let proxy_url = (*proxy_url).clone();
          let client = state.client.clone();
          Ok(Promised(async move {
            JsResult(
              do_fetch(client, &method, &proxy_url, headers, body.map(reqwest::Body::from)).await.map(JsResponseData),
            )
          }))
        }
      }),
    )
    .expect("create proxy fetch");
    ctx.globals().set("fetch", fetch_fn).expect("override fetch global");
  }

  log::info!("[sgo] Installed proxy (http={http}) -> http://{}/", &*base);
}
