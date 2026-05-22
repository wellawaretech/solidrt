// Replaces Flux.file and Flux.dir with HTTP-backed versions that route through
// the cli's dev server. Flux.write is left alone (cli has no write endpoint;
// dev writes stay device-local).
//
// File vs directory disambiguation uses the cli's X-SRT-Type response header
// (set by server.ts) to avoid ambiguity around .json files.

use flux::attach_body;
use flux::rquickjs::{
  function::MutFn, promise::Promised, Array, Ctx, Function, IntoJs, JsLifetime, Object, Value,
};
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

fn build_proxy_file<'js>(
  ctx: Ctx<'js>,
  path: String,
) -> flux::rquickjs::Result<Object<'js>> {
  let state = ctx
    .userdata::<ProxyState>()
    .expect("proxy state")
    .clone();
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
        let resp = client.get(&*url).send().await.map_err(http_err)?;
        let status = resp.status();
        if !status.is_success() {
          return Err(http_err(format!(
            "HTTP {} for {}",
            status.as_u16(),
            &*url
          )));
        }
        resp.bytes().await.map(|b| b.to_vec()).map_err(http_err)
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
            Ok(resp) if resp.status().is_success() => {
              header_str(&resp, SRT_TYPE_HEADER) != Some("directory")
            }
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
            return Err(http_err(format!(
              "stat HTTP {} for {}",
              resp.status().as_u16(),
              &*url
            )));
          }
          let size = header_str(&resp, "content-length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
          Ok::<ProxyStat, flux::rquickjs::Error>(ProxyStat { size })
        }))
      }
    }),
  )
  .expect("create file.stat");
  obj.set("stat", stat_fn)?;

  Ok(obj)
}

fn build_proxy_dir<'js>(
  ctx: Ctx<'js>,
  path: String,
) -> flux::rquickjs::Result<Object<'js>> {
  let state = ctx
    .userdata::<ProxyState>()
    .expect("proxy state")
    .clone();
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
            return Err(http_err(format!(
              "entries HTTP {} for {}",
              resp.status().as_u16(),
              &*url
            )));
          }
          let body = resp.bytes().await.map_err(http_err)?;
          let arr: Vec<serde_json::Value> = serde_json::from_slice(&body)
            .map_err(|e| http_err(format!("invalid dir listing: {e}")))?;
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
            Ok(resp) if resp.status().is_success() => {
              header_str(&resp, SRT_TYPE_HEADER) == Some("directory")
            }
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

pub fn install_proxy(ctx: Ctx<'_>, dev_server: String) {
  let client = reqwest::Client::builder()
    .user_agent("lattice-go-proxy")
    .build()
    .expect("build proxy http client");

  let base = Rc::new(dev_server);
  ctx
    .store_userdata(ProxyState {
      base: base.clone(),
      client: Rc::new(client),
    })
    .expect("store proxy state");

  let flux: Object = ctx
    .globals()
    .get("Flux")
    .expect("Flux global must be set before installing proxy");

  let file_fn =
    Function::new(ctx.clone(), build_proxy_file).expect("create proxy Flux.file");
  flux.set("file", file_fn).expect("override Flux.file");

  let dir_fn =
    Function::new(ctx.clone(), build_proxy_dir).expect("create proxy Flux.dir");
  flux.set("dir", dir_fn).expect("override Flux.dir");

  log::info!("[sgo] Installed Flux.file/dir proxy -> http://{}/", &*base);
}