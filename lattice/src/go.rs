use flux::attach_body;
use flux::rquickjs::{
  function::MutFn, promise::Promised, Array, Ctx, Function, IntoJs, JsLifetime, Object, Value,
};
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::OnceCell;

pub type DevServerCell = Arc<OnceCell<String>>;

pub fn start(
  handle: &tokio::runtime::Handle,
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
) {
  handle.spawn(async move { spawn_go_udp_discovery(tx, dev_server).await });
}

async fn spawn_go_ws(
  dev_server_addr: String,
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
) {
  use futures_util::{SinkExt, StreamExt};

  log::info!("[sgo] Connecting to ws://{}...", dev_server_addr);

  let uri = http::Uri::builder()
    .scheme("ws")
    .authority(dev_server_addr.as_str())
    .path_and_query("/")
    .build()
    .expect("invalid dev server URI");

  loop {
    let (mut client, _) = loop {
      match tokio_websockets::ClientBuilder::from_uri(uri.clone())
        .connect()
        .await
      {
        Ok(conn) => break conn,
        Err(e) => {
          log::warn!("[sgo] Connection failed: {e}, retrying in 3s...");
          tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
      }
    };

    log::info!("[sgo] Connected to ws://{dev_server_addr}");

    // Publish the dev server address so the next engine build can install
    // the file/dir proxy. set() returns Err if already set; ignore.
    let _ = dev_server.set(dev_server_addr.clone());

    let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
    let info = format!(
      r#"{{"type":"info","platform":"{}","version":"{version}"}}"#,
      std::env::consts::OS,
    );
    let _ = client.send(tokio_websockets::Message::text(info)).await;

    while let Some(Ok(msg)) = client.next().await {
      if let Some(text) = msg.as_text() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
          match json.get("type").and_then(|t| t.as_str()) {
            Some("reload") => {
              if let Some(code) = json.get("code").and_then(|c| c.as_str()) {
                let _ = tx.send(crate::EngineCmd::Reload(code.to_string()));
              }
            }
            Some("stop") => {
              let _ = tx.send(crate::EngineCmd::Stop);
            }
            _ => {}
          }
        }
      }
    }

    log::warn!("[sgo] Connection lost, reconnecting in 3s...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}

const DEV_SERVER_PORT: u16 = 15194;

async fn spawn_go_udp_discovery(
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
) {
  use tokio::net::UdpSocket;

  log::info!("[sgo] Starting UDP discovery on port {DEV_SERVER_PORT}...");

  let sock = match UdpSocket::bind("0.0.0.0:0").await {
    Ok(s) => s,
    Err(e) => {
      log::error!("[sgo] UDP bind failed: {e}");
      return;
    }
  };
  if let Err(e) = sock.set_broadcast(true) {
    log::error!("[sgo] UDP set_broadcast failed: {e}");
    return;
  }

  let mut buf = [0u8; 64];
  loop {
    let dest = format!("255.255.255.255:{DEV_SERVER_PORT}");
    if let Err(e) = sock.send_to(b"SRT_DISCOVER", &dest).await {
      log::warn!("[sgo] UDP send failed: {e}");
    }

    match tokio::time::timeout(
      std::time::Duration::from_secs(2),
      sock.recv_from(&mut buf),
    )
    .await
    {
      Ok(Ok((len, addr))) => {
        let msg = std::str::from_utf8(&buf[..len]).unwrap_or("");
        if msg == "SRT_SERVER" {
          let server_addr = format!("{}:{DEV_SERVER_PORT}", addr.ip());
          log::info!("[sgo] Discovered dev server at {server_addr}");
          spawn_go_ws(server_addr, tx, dev_server).await;
          return;
        }
      }
      Ok(Err(e)) => log::warn!("[sgo] UDP recv error: {e}"),
      Err(_) => log::debug!("[sgo] No dev server found, retrying..."),
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}

// --- Proxy plugin -------------------------------------------------
//
// Replaces Flux.file and Flux.dir with HTTP-backed versions that route
// through the cli's dev server. Flux.write is left alone (cli has no
// write endpoint; dev writes stay device-local).
//
// File vs directory disambiguation uses the cli's X-SRT-Type response
// header (set by server.ts) to avoid ambiguity around .json files.

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

fn header_str<'a>(
  resp: &'a reqwest::Response,
  name: &str,
) -> Option<&'a str> {
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
