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
use flux::{attach_body, do_fetch, JsResponseData, JsResult, SeekableReader, SeekableSource};
use std::io::{self, Read, Seek, SeekFrom};
use std::rc::Rc;
use std::sync::mpsc;

const SRT_TYPE_HEADER: &str = "x-srt-type";

#[derive(Clone, JsLifetime)]
struct ProxyState {
  #[qjs(skip_trace)]
  base: Rc<String>,
  #[qjs(skip_trace)]
  client: Rc<reqwest::Client>,
  // Handle to the dedicated fetch worker, cloned into each streamed file's
  // reader. Rc so the JS-thread state holds one; a reader takes an owned clone.
  #[qjs(skip_trace)]
  fetch: Rc<FetchHandle>,
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

/// How many bytes to pull per network request. A streaming decoder issues many
/// small sequential reads; fetching a chunk at a time and serving reads from it
/// keeps the request count sane. A seek outside the buffer refetches.
const CHUNK: usize = 256 * 1024;

/// A range or size request for the dev server, carrying a reply channel.
enum FetchReq {
  Range { start: u64, len: usize, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  Size { reply: mpsc::Sender<Result<u64, String>> },
}

/// A handle to the proxy's dedicated fetch worker (see `spawn_fetcher`). Cloning
/// it is cheap (an `mpsc::Sender`) and `Send`, so a streaming reader can carry
/// one and block for answers from any thread.
#[derive(Clone)]
struct FetchHandle {
  tx: mpsc::Sender<(String, FetchReq)>,
}

impl FetchHandle {
  fn request<T>(&self, url: &str, make: impl FnOnce(mpsc::Sender<Result<T, String>>) -> FetchReq) -> io::Result<T> {
    let (reply_tx, reply_rx) = mpsc::channel();
    self.tx.send((url.to_string(), make(reply_tx))).map_err(|_| io::Error::other("proxy fetch worker gone"))?;
    reply_rx.recv().map_err(|_| io::Error::other("proxy fetch reply lost"))?.map_err(io::Error::other)
  }

  fn range(&self, url: &str, start: u64, len: usize) -> io::Result<Vec<u8>> {
    self.request(url, |reply| FetchReq::Range { start, len, reply })
  }

  fn size(&self, url: &str) -> io::Result<u64> {
    self.request(url, |reply| FetchReq::Size { reply })
  }
}

/// Spawn the proxy's fetch worker: one OS thread with its own runtime and
/// reqwest client, serving range/size requests over a channel. It exists because
/// SDL's on-load header parse reads the byte source SYNCHRONOUSLY on the calling
/// thread - which is the app's runtime thread - and blocking that thread to
/// drive a request on the SAME runtime dead-locks (or panics). Routing the I/O
/// to an independent thread lets a reader block for the answer from any thread
/// (the runtime thread during load, SDL's decode thread during playback). The
/// thread exits once every `FetchHandle` (the proxy state and all live readers)
/// has dropped.
fn spawn_fetcher() -> FetchHandle {
  let (tx, rx) = mpsc::channel::<(String, FetchReq)>();
  std::thread::Builder::new()
    .name("srt-proxy-fetch".into())
    .spawn(move || {
      let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("build proxy fetch runtime");
      let client = reqwest::Client::new();
      while let Ok((url, req)) = rx.recv() {
        match req {
          FetchReq::Range { start, len, reply } => {
            let _ = reply.send(rt.block_on(fetch_range(&client, &url, start, len)));
          }
          FetchReq::Size { reply } => {
            let _ = reply.send(rt.block_on(fetch_size(&client, &url)));
          }
        }
      }
    })
    .expect("spawn proxy fetch thread");
  FetchHandle { tx }
}

/// Fetch `[start, start+len)` via a range request. Returns fewer bytes at EOF.
async fn fetch_range(client: &reqwest::Client, url: &str, start: u64, len: usize) -> Result<Vec<u8>, String> {
  let range = format!("bytes={}-{}", start, start + len as u64 - 1);
  let resp = client.get(url).header(reqwest::header::RANGE, range).send().await.map_err(|e| e.to_string())?;
  let status = resp.status();
  // 416 (range not satisfiable) means the start is at or past EOF: report it as
  // a clean end of stream (no bytes) rather than an error.
  if status.as_u16() == 416 {
    return Ok(Vec::new());
  }
  if !status.is_success() {
    return Err(format!("range HTTP {} for {}", status.as_u16(), url));
  }
  // 200 means the server ignored the range and sent the whole file.
  let whole = status.as_u16() == 200;
  let body = resp.bytes().await.map_err(|e| e.to_string())?;
  if whole {
    let s = (start as usize).min(body.len());
    let e = s.saturating_add(len).min(body.len());
    Ok(body[s..e].to_vec())
  } else {
    Ok(body.to_vec())
  }
}

/// Fetch the total size via a HEAD (content-length).
async fn fetch_size(client: &reqwest::Client, url: &str) -> Result<u64, String> {
  let resp = client.head(url).send().await.map_err(|e| e.to_string())?;
  if !resp.status().is_success() {
    return Err(format!("head HTTP {} for {}", resp.status().as_u16(), url));
  }
  resp
    .headers()
    .get(reqwest::header::CONTENT_LENGTH)
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.parse::<u64>().ok())
    .ok_or_else(|| format!("missing content-length for {url}"))
}

/// A seekable byte reader backed by HTTP range requests to the dev server, so a
/// proxied file can stream (decode on demand) instead of being pulled whole. It
/// is read from whichever thread SDL_mixer decodes on; the network round-trips
/// go through the fetch worker, so blocking here never touches the app runtime.
struct ProxyReader {
  fetch: FetchHandle,
  url: String,
  pos: u64,
  // Total size, learned lazily from a HEAD (needed for SeekFrom::End).
  size: Option<u64>,
  // A cached window of the file at [buf_start, buf_start + buf.len()).
  buf: Vec<u8>,
  buf_start: u64,
}

impl ProxyReader {
  fn new(fetch: FetchHandle, url: String) -> Self {
    ProxyReader { fetch, url, pos: 0, size: None, buf: Vec::new(), buf_start: 0 }
  }

  fn ensure_size(&mut self) -> io::Result<u64> {
    if let Some(s) = self.size {
      return Ok(s);
    }
    let s = self.fetch.size(&self.url)?;
    self.size = Some(s);
    Ok(s)
  }
}

impl Read for ProxyReader {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }
    let covered = self.pos >= self.buf_start && self.pos < self.buf_start + self.buf.len() as u64;
    if !covered {
      self.buf = self.fetch.range(&self.url, self.pos, CHUNK)?;
      self.buf_start = self.pos;
    }
    let off = (self.pos - self.buf_start) as usize;
    if off >= self.buf.len() {
      return Ok(0); // EOF
    }
    let n = (self.buf.len() - off).min(buf.len());
    buf[..n].copy_from_slice(&self.buf[off..off + n]);
    self.pos += n as u64;
    Ok(n)
  }
}

impl Seek for ProxyReader {
  fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
    let newpos = match pos {
      SeekFrom::Start(o) => o,
      SeekFrom::Current(o) => self.pos.checked_add_signed(o).ok_or_else(|| io::Error::other("seek out of range"))?,
      SeekFrom::End(o) => {
        let size = self.ensure_size()?;
        size.checked_add_signed(o).ok_or_else(|| io::Error::other("seek out of range"))?
      }
    };
    self.pos = newpos;
    Ok(newpos)
  }
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

  // Attach a range-backed seekable source, mirroring the local file()'s disk
  // opener, so a streamed file rides this proxy: the byte source pulls from the
  // dev server on demand instead of local disk.
  let range_url = url.clone();
  let range_fetch = (*state.fetch).clone();
  SeekableSource::attach(
    &ctx,
    &obj,
    Rc::new(move || Ok(Box::new(ProxyReader::new(range_fetch.clone(), (*range_url).clone())) as SeekableReader)),
  )?;

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
  ctx
    .store_userdata(ProxyState { base: base.clone(), client: Rc::new(client), fetch: Rc::new(spawn_fetcher()) })
    .expect("store proxy state");

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
