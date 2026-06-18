//! Engine-free filesystem core.
//!
//! The scripting-engine-independent half of `flux:fs` (the `file()`/`dir()`
//! references): the actual disk operations and the owned `StatInfo` result type.
//! It names no scripting-engine types; the marshalling layer
//! (`plugins/flux/file.rs`, `plugins/flux/dir.rs`) decodes JS args into these
//! calls, holds `PendingOps` around them, and encodes the results back to JS.
//! Destined for the `forge` crate (see REDESIGN.md).

/// A file's metadata, as returned by `stat`. `file_type` is `"file"`,
/// `"directory"`, `"symlink"`, or `"other"`; `mtime_ms` is the modification time
/// in milliseconds since the Unix epoch, absent if the platform/file has none.
pub struct StatInfo {
  pub size: u64,
  pub file_type: &'static str,
  pub mtime_ms: Option<i64>,
}

/// Read a file's whole contents.
pub async fn read(path: &str) -> Result<Vec<u8>, String> {
  tokio::fs::read(path).await.map_err(|e| format!("read {path}: {e}"))
}

/// Write bytes to a file, truncating any existing contents.
pub async fn write(path: &str, bytes: &[u8]) -> Result<(), String> {
  tokio::fs::write(path, bytes).await.map_err(|e| format!("write {path}: {e}"))
}

/// Whether `path` exists and is a regular file. A missing path (or any stat
/// error) is reported as `false`, not an error.
pub async fn file_exists(path: &str) -> bool {
  tokio::fs::metadata(path).await.map(|m| m.is_file()).unwrap_or(false)
}

/// Stat a path. Errors if it does not exist or cannot be read.
pub async fn stat(path: &str) -> Result<StatInfo, String> {
  let meta = tokio::fs::metadata(path).await.map_err(|e| format!("stat {path}: {e}"))?;
  Ok(StatInfo { size: meta.len(), file_type: type_str(meta.file_type()), mtime_ms: mtime_ms(&meta) })
}

/// Whether `path` exists and is a directory. A missing path (or any stat error)
/// is reported as `false`, not an error.
pub async fn dir_exists(path: &str) -> bool {
  tokio::fs::metadata(path).await.map(|m| m.is_dir()).unwrap_or(false)
}

/// List a directory's entries as `(name, type)` pairs, where type is the same
/// set as `StatInfo::file_type`.
pub async fn read_dir(path: &str) -> Result<Vec<(String, &'static str)>, String> {
  let mut entries = tokio::fs::read_dir(path).await.map_err(|e| format!("read dir {path}: {e}"))?;
  let mut out = Vec::new();
  while let Some(entry) = entries.next_entry().await.map_err(|e| format!("read dir {path}: {e}"))? {
    let name = entry.file_name().to_string_lossy().into_owned();
    let ft = entry.file_type().await.map_err(|e| format!("read dir {path}: {e}"))?;
    out.push((name, type_str(ft)));
  }
  Ok(out)
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