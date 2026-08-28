//! Engine-free disk cache core.
//!
//! A persistent, size-capped store of opaque entries keyed by string. Knows
//! nothing about HTTP: the metadata blob belongs to the consumer (forge/fetch
//! stores its response snapshot there). One file per entry, named by the
//! blake3 hash of the key: a length-prefixed metadata blob followed by the
//! body bytes. Writes stream through a uniquely-named temp file and commit
//! (rename into place) only on clean body completion, so a failed or
//! abandoned download leaves no trace. Eviction is LRU by file mtime (bumped
//! on lookup), enforced lazily after each commit.

use bytes::Bytes;
use futures_core::Stream;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::stream::ByteStream;

/// Guards the header read against a corrupt or foreign file claiming an
/// absurd metadata length.
const MAX_META_BYTES: u32 = 1024 * 1024;

pub struct Cache {
  dir: PathBuf,
  max_bytes: u64,
  max_entry_bytes: u64,
}

enum WriteMsg {
  Chunk(Bytes),
  Commit,
}

impl Cache {
  /// A cache over `dir` (created lazily on first write), holding at most
  /// `max_bytes` of committed entries. A single entry may use at most a
  /// quarter of the cap, so one giant download cannot evict everything.
  pub fn new(dir: PathBuf, max_bytes: u64) -> Self {
    Self { dir, max_bytes, max_entry_bytes: max_bytes / 4 }
  }

  fn entry_path(&self, key: &str) -> PathBuf {
    self.dir.join(blake3::hash(key.as_bytes()).to_hex().as_str())
  }

  /// Look up an entry: the consumer's metadata blob plus the body as a
  /// stream. Bumps the entry's mtime (the LRU clock). A corrupt entry is
  /// removed and treated as a miss.
  ///
  /// The open and header read are sync on purpose: they are tiny, and the
  /// same handle then converts to a tokio file for the streamed body.
  pub async fn lookup(&self, key: &str) -> Option<(Vec<u8>, ByteStream)> {
    let path = self.entry_path(key);
    // Opened writable only for the mtime bump: Windows refuses to set times
    // on a read-only handle (silently, through the `let _`), which turned the
    // LRU clock into write order there. Nothing writes through the handle.
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(&path).ok()?;
    match read_header(&mut file) {
      Ok(meta) => {
        let _ = file.set_modified(std::time::SystemTime::now());
        let body: ByteStream = Box::pin(tokio_util::io::ReaderStream::new(tokio::fs::File::from_std(file)));
        Some((meta, body))
      }
      Err(_) => {
        let _ = tokio::fs::remove_file(&path).await;
        None
      }
    }
  }

  /// Tee `body` into the cache: the returned stream is drained by the
  /// consumer as usual while a background task writes the chunks to a temp
  /// file. The entry is committed only when the stream ends cleanly; a body
  /// error, an abandoned stream, or an entry over the per-entry cap leaves
  /// nothing behind.
  pub fn store(&self, key: &str, meta: Vec<u8>, body: ByteStream) -> ByteStream {
    let (tx, rx) = mpsc::unbounded_channel::<WriteMsg>();
    let dir = self.dir.clone();
    let dest = self.entry_path(key);
    let max_bytes = self.max_bytes;
    let max_entry_bytes = self.max_entry_bytes;
    tokio::spawn(async move {
      if let Err(e) = write_entry(dir, dest, meta, rx, max_entry_bytes, max_bytes).await {
        log::warn!("Cache write failed: {e}");
      }
    });
    Box::pin(TeeStream { inner: body, tx: Some(tx) })
  }
}

fn read_header(file: &mut std::fs::File) -> io::Result<Vec<u8>> {
  use std::io::Read;
  let mut len_buf = [0u8; 4];
  file.read_exact(&mut len_buf)?;
  let len = u32::from_le_bytes(len_buf);
  if len > MAX_META_BYTES {
    return Err(io::Error::other("oversized cache metadata"));
  }
  let mut meta = vec![0u8; len as usize];
  file.read_exact(&mut meta)?;
  Ok(meta)
}

/// A committed entry as `scan` lists it: the consumer's metadata blob plus
/// the size on disk (header and body) and last-use time (the LRU clock).
pub struct ScannedEntry {
  pub meta: Vec<u8>,
  pub size: u64,
  pub modified: std::time::SystemTime,
}

/// List a cache directory's committed entries, for inspection tooling (sync:
/// callers browse a directory, they do not sit on the request path). A
/// missing directory is an empty cache. In-flight `.tmp` files are skipped,
/// and unreadable or corrupt files are ignored rather than removed: a scan
/// must not race an active writer.
pub fn scan(dir: &Path) -> Vec<ScannedEntry> {
  let Ok(read_dir) = std::fs::read_dir(dir) else { return Vec::new() };
  let mut entries = Vec::new();
  for entry in read_dir.flatten() {
    let path = entry.path();
    if path.extension().is_some_and(|e| e == "tmp") {
      continue;
    }
    let Ok(md) = entry.metadata() else { continue };
    if !md.is_file() {
      continue;
    }
    let Ok(mut file) = std::fs::File::open(&path) else { continue };
    let Ok(meta) = read_header(&mut file) else { continue };
    entries.push(ScannedEntry {
      meta,
      size: md.len(),
      modified: md.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
    });
  }
  entries
}

/// Passthrough stream that mirrors each chunk to the writer task. Dropping
/// the sender without `Commit` (consumer stopped reading, or the body
/// errored) makes the writer discard the temp file.
struct TeeStream {
  inner: ByteStream,
  tx: Option<mpsc::UnboundedSender<WriteMsg>>,
}

impl Stream for TeeStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    let this = self.get_mut();
    let polled = this.inner.as_mut().poll_next(cx);
    match &polled {
      Poll::Ready(Some(Ok(chunk))) => {
        if let Some(tx) = &this.tx {
          // Bytes clones share the buffer; a send failure means the writer
          // aborted (per-entry cap), which is not the consumer's problem.
          let _ = tx.send(WriteMsg::Chunk(chunk.clone()));
        }
      }
      Poll::Ready(Some(Err(_))) => {
        this.tx = None;
      }
      Poll::Ready(None) => {
        if let Some(tx) = this.tx.take() {
          let _ = tx.send(WriteMsg::Commit);
        }
      }
      Poll::Pending => {}
    }
    polled
  }
}

async fn write_entry(
  dir: PathBuf,
  dest: PathBuf,
  meta: Vec<u8>,
  mut rx: mpsc::UnboundedReceiver<WriteMsg>,
  max_entry_bytes: u64,
  max_bytes: u64,
) -> io::Result<()> {
  tokio::fs::create_dir_all(&dir).await?;
  // Unique per writer, so concurrent stores of one key cannot corrupt each
  // other's temp file; the last committed rename wins.
  static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
  let tmp = dest.with_extension(format!("{}-{}.tmp", std::process::id(), TMP_SEQ.fetch_add(1, Ordering::Relaxed)));

  let result = write_tmp(&tmp, meta, &mut rx, max_entry_bytes).await;
  match result {
    Ok(true) => {
      // Rename over an existing entry; Windows needs the target gone first.
      if tokio::fs::rename(&tmp, &dest).await.is_err() {
        let _ = tokio::fs::remove_file(&dest).await;
        tokio::fs::rename(&tmp, &dest).await?;
      }
      evict(&dir, max_bytes).await;
      Ok(())
    }
    Ok(false) => {
      let _ = tokio::fs::remove_file(&tmp).await;
      Ok(())
    }
    Err(e) => {
      let _ = tokio::fs::remove_file(&tmp).await;
      Err(e)
    }
  }
}

/// Returns Ok(true) on a committed body, Ok(false) on an abandoned or
/// oversized one. The file handle closes on return, before the rename.
async fn write_tmp(
  tmp: &Path,
  meta: Vec<u8>,
  rx: &mut mpsc::UnboundedReceiver<WriteMsg>,
  max_entry_bytes: u64,
) -> io::Result<bool> {
  let mut file = tokio::fs::File::create(tmp).await?;
  file.write_all(&(meta.len() as u32).to_le_bytes()).await?;
  file.write_all(&meta).await?;
  let mut written: u64 = 0;
  while let Some(msg) = rx.recv().await {
    match msg {
      WriteMsg::Chunk(chunk) => {
        written += chunk.len() as u64;
        if written > max_entry_bytes {
          return Ok(false);
        }
        file.write_all(&chunk).await?;
      }
      WriteMsg::Commit => {
        file.flush().await?;
        return Ok(true);
      }
    }
  }
  Ok(false)
}

/// Drop oldest-mtime entries until the committed total fits the cap. In-flight
/// `.tmp` files belong to active writers and are skipped.
async fn evict(dir: &Path, max_bytes: u64) {
  let Ok(mut read_dir) = tokio::fs::read_dir(dir).await else { return };
  let mut entries: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
  while let Ok(Some(entry)) = read_dir.next_entry().await {
    let path = entry.path();
    if path.extension().is_some_and(|e| e == "tmp") {
      continue;
    }
    let Ok(md) = entry.metadata().await else { continue };
    if !md.is_file() {
      continue;
    }
    entries.push((path, md.len(), md.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
  }
  let total: u64 = entries.iter().map(|e| e.1).sum();
  if total <= max_bytes {
    return;
  }
  entries.sort_by_key(|e| e.2);
  let mut excess = total - max_bytes;
  for (path, len, _) in entries {
    if excess == 0 {
      break;
    }
    if tokio::fs::remove_file(&path).await.is_ok() {
      excess = excess.saturating_sub(len);
    }
  }
}
