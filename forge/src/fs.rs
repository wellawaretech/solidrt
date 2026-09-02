//! Engine-free filesystem core.
//!
//! The scripting-engine-independent half of `flux:fs` (the `file()`/`dir()`
//! references): the actual disk operations and the owned `StatInfo` result type.
//! It names no scripting-engine types; the marshalling layer (flux
//! `forge_plugins/file.rs` and `forge_plugins/dir.rs`) decodes JS args into
//! these calls, holds `PendingOps` around them, and encodes the results back
//! to JS.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::RwLock;

use crate::seek::SeekableReader;
use crate::Value;

/// A file's metadata, as returned by `stat`. `file_type` is `"file"`,
/// `"directory"`, `"symlink"`, or `"other"`; `mtime_ms` is the modification time
/// in milliseconds since the Unix epoch, absent if the platform/file has none.
pub struct StatInfo {
  pub size: u64,
  pub file_type: &'static str,
  pub mtime_ms: Option<i64>,
}

/// `{ size, type, mtime? }`; `mtime` is omitted (not null) when the platform
/// has none.
impl From<StatInfo> for Value {
  fn from(s: StatInfo) -> Value {
    let mut m = vec![("size".to_string(), Value::from(s.size)), ("type".to_string(), Value::from(s.file_type))];
    if let Some(mtime) = s.mtime_ms {
      m.push(("mtime".to_string(), Value::from(mtime)));
    }
    Value::Map(m)
  }
}

/// One `read_dir` entry: the file name and its type string (`"file"`,
/// `"directory"`, ...).
pub struct DirEntry {
  pub name: String,
  pub kind: &'static str,
}

impl From<DirEntry> for Value {
  fn from(e: DirEntry) -> Value {
    Value::map([("name", Value::from(e.name)), ("type", Value::from(e.kind))])
  }
}

/// Where an app's immutable assets live (okf/plans/client-storage-updates.md,
/// stage 3): a directory containing an `assets/` tree (an installed version
/// dir, a pack folder), or ranges inside a packed executable image (the
/// single-file trailer), read by offset without unpacking.
pub enum AssetsBase {
  Dir(PathBuf),
  Packed {
    exe: PathBuf,
    /// Manifest path -> (absolute file offset, length) in `exe`.
    index: HashMap<String, (u64, u64)>,
  },
}

// The assets mount: when set, relative paths under "assets/" and "isolates/"
// (the app's isolate bundles, delivered as manifest assets) resolve through
// the AssetsBase instead of the process cwd; the app's cwd stays its mutable
// data sandbox. Unset (plain scripts, dev proxy), paths resolve as-is.
static ASSETS_BASE: RwLock<Option<AssetsBase>> = RwLock::new(None);

/// Set or clear the assets mount.
pub fn set_assets_base(base: Option<AssetsBase>) {
  *ASSETS_BASE.write().expect("assets base lock") = base;
}

/// The manifest-delivered trees: `assets/` (the project's asset folder) and
/// `isolates/` (isolate bundles). Both are immutable and read through the
/// mount while one is set.
pub fn is_asset_path(path: &str) -> bool {
  ["assets", "isolates"].iter().any(|root| path == *root || path.starts_with(&format!("{root}/")))
}

// A mounted asset path resolves to a plain file, a byte range inside the
// packed executable, or nothing (a packed path the index does not know).
// Non-asset paths (and everything while unmounted) stay plain.
enum Resolved {
  Plain(PathBuf),
  Slice(PathBuf, u64, u64),
  Missing,
}

fn resolve(path: &str) -> Resolved {
  if is_asset_path(path) {
    match ASSETS_BASE.read().expect("assets base lock").as_ref() {
      Some(AssetsBase::Dir(base)) => return Resolved::Plain(base.join(path)),
      Some(AssetsBase::Packed { exe, index }) => {
        return match index.get(path) {
          Some(&(offset, len)) => Resolved::Slice(exe.clone(), offset, len),
          None => Resolved::Missing,
        }
      }
      None => {}
    }
  }
  Resolved::Plain(PathBuf::from(path))
}

/// Refuse mutation of mounted assets: an installed version is immutable, so
/// writes under `assets/` error while the mount is active.
fn check_writable(path: &str, what: &str) -> Result<(), String> {
  if is_asset_path(path) && ASSETS_BASE.read().expect("assets base lock").is_some() {
    return Err(format!("{what} {path}: assets are read-only"));
  }
  Ok(())
}

// Read `length` bytes at `offset` inside `file` (absolute offsets).
async fn read_slice(file: &PathBuf, offset: u64, length: u64, path: &str) -> Result<Vec<u8>, String> {
  use tokio::io::{AsyncReadExt, AsyncSeekExt};
  let err = |e| format!("read {path}: {e}");
  let mut f = tokio::fs::File::open(file).await.map_err(err)?;
  f.seek(std::io::SeekFrom::Start(offset)).await.map_err(err)?;
  let mut buf = vec![0u8; usize::try_from(length).map_err(|_| format!("read {path}: too large"))?];
  f.read_exact(&mut buf).await.map_err(err)?;
  Ok(buf)
}

fn missing(path: &str, what: &str) -> String {
  format!("{what} {path}: no such packed asset")
}

/// Read a file's whole contents, blocking. For callers that hold no async
/// context (a module resolver called from a synchronous binding); everything
/// else uses `read`.
pub fn read_sync(path: &str) -> Result<Vec<u8>, String> {
  match resolve(path) {
    Resolved::Plain(p) => std::fs::read(p).map_err(|e| format!("read {path}: {e}")),
    Resolved::Slice(file, offset, len) => {
      let err = |e| format!("read {path}: {e}");
      let mut f = std::fs::File::open(&file).map_err(err)?;
      f.seek(SeekFrom::Start(offset)).map_err(err)?;
      let mut buf = vec![0u8; usize::try_from(len).map_err(|_| format!("read {path}: too large"))?];
      f.read_exact(&mut buf).map_err(err)?;
      Ok(buf)
    }
    Resolved::Missing => Err(missing(path, "read")),
  }
}

/// Read a file's whole contents.
pub async fn read(path: &str) -> Result<Vec<u8>, String> {
  match resolve(path) {
    Resolved::Plain(p) => tokio::fs::read(p).await.map_err(|e| format!("read {path}: {e}")),
    Resolved::Slice(file, offset, len) => read_slice(&file, offset, len, path).await,
    Resolved::Missing => Err(missing(path, "read")),
  }
}

/// Read `length` bytes starting at byte `offset`. Exact: a range extending past
/// end-of-file is an error, not a short read (callers clamp against
/// `stat().size` first, which is what HTTP 206 needs anyway).
pub async fn read_range(path: &str, offset: u64, length: u64) -> Result<Vec<u8>, String> {
  match resolve(path) {
    Resolved::Plain(p) => {
      use tokio::io::{AsyncReadExt, AsyncSeekExt};
      let err = |e| format!("read {path} at {offset}+{length}: {e}");
      let mut file = tokio::fs::File::open(p).await.map_err(|e| format!("open {path}: {e}"))?;
      file.seek(std::io::SeekFrom::Start(offset)).await.map_err(err)?;
      let mut buf = vec![0u8; usize::try_from(length).map_err(|_| format!("read {path}: length {length} too large"))?];
      file.read_exact(&mut buf).await.map_err(err)?;
      Ok(buf)
    }
    Resolved::Slice(file, start, len) => {
      if offset.checked_add(length).map(|end| end > len).unwrap_or(true) {
        return Err(format!("read {path} at {offset}+{length}: past end of asset ({len} bytes)"));
      }
      read_slice(&file, start + offset, length, path).await
    }
    Resolved::Missing => Err(missing(path, "read")),
  }
}

/// Write bytes to a file, truncating any existing contents.
pub async fn write(path: &str, bytes: &[u8]) -> Result<(), String> {
  check_writable(path, "write")?;
  tokio::fs::write(path, bytes).await.map_err(|e| format!("write {path}: {e}"))
}

/// Remove a file. Missing is not an error: the caller wants it gone, and it
/// is (a record dropped twice, a cleanup after a crash).
pub async fn remove(path: &str) -> Result<(), String> {
  check_writable(path, "remove")?;
  match tokio::fs::remove_file(path).await {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(format!("remove {path}: {e}")),
  }
}

/// Append bytes to a file, creating it if missing.
pub async fn append(path: &str, bytes: &[u8]) -> Result<(), String> {
  use tokio::io::AsyncWriteExt;
  check_writable(path, "append")?;
  let err = |e| format!("append {path}: {e}");
  let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(path).await.map_err(err)?;
  file.write_all(bytes).await.map_err(err)
}

/// Open a file for seekable, on-demand reads (e.g. feeding a streaming audio
/// decoder) without pulling it into memory. Sync and a plain `std::fs::File`
/// because the handle is read from a foreign decode thread, not the tokio
/// runtime.
pub fn open_seekable(path: &str) -> Result<SeekableReader, String> {
  match resolve(path) {
    Resolved::Plain(p) => {
      let file = std::fs::File::open(p).map_err(|e| format!("open {path}: {e}"))?;
      Ok(Box::new(file))
    }
    Resolved::Slice(file, offset, len) => {
      let file = std::fs::File::open(file).map_err(|e| format!("open {path}: {e}"))?;
      Ok(Box::new(FileWindow { file, start: offset, len, pos: 0 }))
    }
    Resolved::Missing => Err(missing(path, "open")),
  }
}

/// Whether `path` exists and is a regular file. A missing path (or any stat
/// error) is reported as `false`, not an error.
pub async fn file_exists(path: &str) -> bool {
  match resolve(path) {
    Resolved::Plain(p) => tokio::fs::metadata(p).await.map(|m| m.is_file()).unwrap_or(false),
    Resolved::Slice(..) => true,
    Resolved::Missing => false,
  }
}

/// Stat a path. Errors if it does not exist or cannot be read.
pub async fn stat(path: &str) -> Result<StatInfo, String> {
  match resolve(path) {
    Resolved::Plain(p) => {
      let meta = tokio::fs::metadata(p).await.map_err(|e| format!("stat {path}: {e}"))?;
      Ok(StatInfo { size: meta.len(), file_type: type_str(meta.file_type()), mtime_ms: mtime_ms(&meta) })
    }
    Resolved::Slice(_, _, len) => Ok(StatInfo { size: len, file_type: "file", mtime_ms: None }),
    Resolved::Missing => {
      // A packed "directory" is any index prefix; report it so dir().exists()
      // and stat agree with the synthesized listings.
      if packed_dir_entries(path).is_some_and(|e| !e.is_empty()) {
        Ok(StatInfo { size: 0, file_type: "directory", mtime_ms: None })
      } else {
        Err(missing(path, "stat"))
      }
    }
  }
}

/// Create a directory, including any missing parents. Succeeds if it already
/// exists.
pub async fn create_dir(path: &str) -> Result<(), String> {
  check_writable(path, "create dir")?;
  tokio::fs::create_dir_all(path).await.map_err(|e| format!("create dir {path}: {e}"))
}

/// Whether `path` exists and is a directory. A missing path (or any stat error)
/// is reported as `false`, not an error.
pub async fn dir_exists(path: &str) -> bool {
  match resolve(path) {
    Resolved::Plain(p) => tokio::fs::metadata(p).await.map(|m| m.is_dir()).unwrap_or(false),
    Resolved::Slice(..) => false,
    Resolved::Missing => packed_dir_entries(path).is_some_and(|e| !e.is_empty()),
  }
}

/// The canonical absolute path: symlinks resolved, `.`/`..` collapsed, the
/// spelling the OS reports. Errors if the path does not exist. What a registry
/// keys on, so two processes agree on a path and not on its spelling.
pub async fn realpath(path: &str) -> Result<String, String> {
  let canonical = tokio::fs::canonicalize(path).await.map_err(|e| format!("realpath {path}: {e}"))?;
  Ok(strip_verbatim(canonical))
}

// Windows canonicalize yields verbatim (`\\?\C:\...`) paths; strip the prefix so
// the result matches what everything else (node's realpath, the shell) prints.
fn strip_verbatim(path: PathBuf) -> String {
  let s = path.to_string_lossy().into_owned();
  if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
    return format!(r"\\{rest}");
  }
  s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// List a directory's entries; `DirEntry::kind` is the same set as
/// `StatInfo::file_type`.
pub async fn read_dir(path: &str) -> Result<Vec<DirEntry>, String> {
  match resolve(path) {
    Resolved::Plain(p) => {
      let mut entries = tokio::fs::read_dir(p).await.map_err(|e| format!("read dir {path}: {e}"))?;
      let mut out = Vec::new();
      while let Some(entry) = entries.next_entry().await.map_err(|e| format!("read dir {path}: {e}"))? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let ft = entry.file_type().await.map_err(|e| format!("read dir {path}: {e}"))?;
        out.push(DirEntry { name, kind: type_str(ft) });
      }
      Ok(out)
    }
    Resolved::Slice(..) => Err(format!("read dir {path}: not a directory")),
    Resolved::Missing => match packed_dir_entries(path) {
      Some(entries) if !entries.is_empty() => {
        Ok(entries.into_iter().map(|(name, kind)| DirEntry { name, kind }).collect())
      }
      _ => Err(missing(path, "read dir")),
    },
  }
}

// Synthesize a packed-index directory listing: the immediate children of
// `path` across every indexed file path. None when no packed mount is active.
fn packed_dir_entries(path: &str) -> Option<Vec<(String, &'static str)>> {
  if !is_asset_path(path) {
    return None;
  }
  let lock = ASSETS_BASE.read().expect("assets base lock");
  let Some(AssetsBase::Packed { index, .. }) = lock.as_ref() else { return None };
  let prefix = format!("{}/", path.trim_end_matches('/'));
  let mut out: Vec<(String, &'static str)> = Vec::new();
  for key in index.keys() {
    let Some(rest) = key.strip_prefix(&prefix) else { continue };
    let (name, kind) = match rest.split_once('/') {
      Some((dir, _)) => (dir, "directory"),
      None => (rest, "file"),
    };
    if !out.iter().any(|(n, _)| n == name) {
      out.push((name.to_string(), kind));
    }
  }
  out.sort_by(|a, b| a.0.cmp(&b.0));
  Some(out)
}

/// A clamped view of a larger file: reads and seeks confined to
/// `[start, start + len)`. Backs seekable sources for assets packed inside the
/// executable image, so streaming consumers (audio decode) pull ranges on
/// demand without unpacking.
pub(crate) struct FileWindow {
  pub(crate) file: std::fs::File,
  pub(crate) start: u64,
  pub(crate) len: u64,
  pub(crate) pos: u64,
}

impl Read for FileWindow {
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
    let remaining = self.len.saturating_sub(self.pos);
    if remaining == 0 {
      return Ok(0);
    }
    let cap = usize::try_from(remaining).unwrap_or(usize::MAX).min(buf.len());
    self.file.seek(SeekFrom::Start(self.start + self.pos))?;
    let n = self.file.read(&mut buf[..cap])?;
    self.pos += n as u64;
    Ok(n)
  }
}

impl Seek for FileWindow {
  fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
    let target = match pos {
      SeekFrom::Start(n) => Some(n),
      SeekFrom::End(n) => self.len.checked_add_signed(n),
      SeekFrom::Current(n) => self.pos.checked_add_signed(n),
    };
    // Like File: seeking past end is allowed (reads return 0), before start is not.
    let Some(target) = target else {
      return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek before start of asset"));
    };
    self.pos = target;
    Ok(self.pos)
  }
}

fn type_str(ft: std::fs::FileType) -> &'static str {
  if ft.is_file() {
    "file"
  } else if ft.is_dir() {
    "directory"
  } else if ft.is_symlink() {
    "symlink"
  } else {
    "other"
  }
}

fn mtime_ms(meta: &std::fs::Metadata) -> Option<i64> {
  let mtime = meta.modified().ok()?;
  let dur = mtime.duration_since(std::time::SystemTime::UNIX_EPOCH).ok()?;
  i64::try_from(dur.as_millis()).ok()
}

/// What happened to `WatchEvent::path`. `Rename` names the path a file now
/// has (the target of a rename: an editor's atomic save shows up as one); the
/// old name of a rename arrives as `Remove`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchKind {
  Create,
  Modify,
  Remove,
  Rename,
}

impl WatchKind {
  /// The kind's name as the marshalling layers spell it.
  pub fn as_str(self) -> &'static str {
    match self {
      WatchKind::Create => "create",
      WatchKind::Modify => "modify",
      WatchKind::Remove => "remove",
      WatchKind::Rename => "rename",
    }
  }
}

/// One change under a watched directory: the absolute path of the entry and
/// what happened to it. Raw and undebounced; a save typically arrives as
/// several events, and coalescing is the caller's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
  pub kind: WatchKind,
  pub path: String,
}

/// A directory watch: OS change notifications (notify) for one directory,
/// optionally its whole tree, delivered as `WatchEvent`s through `recv`.
/// Dropping the watcher stops the watch.
pub struct DirWatcher {
  _watcher: notify::RecommendedWatcher,
  rx: tokio::sync::mpsc::UnboundedReceiver<WatchEvent>,
}

impl DirWatcher {
  /// Start watching `path` (`recursive`: the tree below it too). `Err` if the
  /// directory does not exist or the OS watch could not be installed.
  pub fn open(path: &str, recursive: bool) -> Result<DirWatcher, String> {
    use notify::Watcher;

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let watched = path.to_string();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
      Ok(event) => {
        for out in translate(event) {
          if tx.send(out).is_err() {
            break;
          }
        }
      }
      Err(e) => log::warn!("Watch {watched}: {e}"),
    })
    .map_err(|e| format!("watch {path}: {e}"))?;
    let mode = if recursive { notify::RecursiveMode::Recursive } else { notify::RecursiveMode::NonRecursive };
    watcher.watch(std::path::Path::new(path), mode).map_err(|e| format!("watch {path}: {e}"))?;
    Ok(DirWatcher { _watcher: watcher, rx })
  }

  /// The next change; `None` once the watch is gone.
  pub async fn recv(&mut self) -> Option<WatchEvent> {
    self.rx.recv().await
  }
}

// Flatten a notify event into our kinds, one per path. Access events are
// noise; a rename's two halves become a Remove of the old name and a Rename
// of the new one, so every event names a path that exists after it (except
// Remove). Anything notify cannot classify is a Modify.
fn translate(event: notify::Event) -> Vec<WatchEvent> {
  use notify::event::{ModifyKind, RenameMode};
  use notify::EventKind;

  let as_string = |p: &PathBuf| p.to_string_lossy().into_owned();
  let all = |kind: WatchKind| -> Vec<WatchEvent> {
    event.paths.iter().map(|p| WatchEvent { kind, path: as_string(p) }).collect()
  };
  match event.kind {
    EventKind::Access(_) => Vec::new(),
    EventKind::Create(_) => all(WatchKind::Create),
    EventKind::Remove(_) => all(WatchKind::Remove),
    EventKind::Modify(ModifyKind::Name(RenameMode::From)) => all(WatchKind::Remove),
    EventKind::Modify(ModifyKind::Name(RenameMode::To)) => all(WatchKind::Rename),
    EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
      let mut out = Vec::new();
      if let Some(from) = event.paths.first() {
        out.push(WatchEvent { kind: WatchKind::Remove, path: as_string(from) });
      }
      if let Some(to) = event.paths.get(1) {
        out.push(WatchEvent { kind: WatchKind::Rename, path: as_string(to) });
      }
      out
    }
    EventKind::Modify(ModifyKind::Name(_)) => all(WatchKind::Rename),
    EventKind::Modify(_) | EventKind::Any | EventKind::Other => all(WatchKind::Modify),
  }
}
