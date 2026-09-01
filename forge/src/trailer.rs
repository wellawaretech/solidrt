//! The packed-executable section trailer: `srt pack` appends an app's payload
//! to a runner image as sections plus a table, and the runner reads its own
//! image back at startup. This is the parsing half of the packed-container
//! contract whose reading half is `fs::AssetsBase::Packed`: [`Trailer::file_index`]
//! produces exactly the index that mount consumes, and sections are read by
//! ranged reads without unpacking anything.
//!
//! Writer: packages/cli/src/packer.ts (packSections). Layout: each section's
//! bytes, then a table of entries, then [table offset u64 LE][entry count
//! u32 LE][magic]. Table entry: [kind u32 LE][offset u64 LE][len u64 LE]
//! [name len u16 LE][name bytes]. Offsets are absolute file offsets. The CLI
//! and runners ship pinned together, so there is no format version; every
//! offset/length is bounds-checked and any mismatch degrades to "no trailer"
//! instead of misparsing.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Section kinds. Which kinds a runner consumes is its own business (fluxrt
/// reads only files; solidrt also reads the manifest and GL libraries).
pub const SECTION_MANIFEST: u32 = 1;
pub const SECTION_FILE: u32 = 2;
pub const SECTION_GL_LIB: u32 = 3;

pub struct Section {
  pub kind: u32,
  pub name: String,
  pub offset: u64,
  pub len: u64,
}

/// A parsed trailer: the image it came from and its sections in table order
/// (which is section order; GL libraries rely on it for preload order).
pub struct Trailer {
  pub exe: PathBuf,
  pub sections: Vec<Section>,
}

/// Parse the trailer of the running executable, if it carries one ending in
/// `magic`. Only the tail and the table are read, not the sections.
pub fn read_own(magic: &[u8]) -> Option<Trailer> {
  read(std::env::current_exe().ok()?, magic)
}

/// Parse the trailer of `exe`. None means "no payload": absent, truncated, or
/// inconsistent trailers all land here rather than misparsing.
pub fn read(exe: PathBuf, magic: &[u8]) -> Option<Trailer> {
  let len = std::fs::metadata(&exe).ok()?.len();
  read_at(exe, 0, len, magic)
}

/// Parse a trailer that sits inside `exe` as the byte range `[base, base+len)`
/// rather than being the whole file: a `.srtapp` stored at an offset in a
/// container (an APK's `assets/app.srtapp` entry). Table offsets inside the
/// payload are relative to its start; the returned sections are rebased to
/// absolute file offsets, so `file_index`/`read_range` (and the
/// `fs::AssetsBase::Packed` mount they feed) work against the container
/// unchanged.
pub fn read_at(exe: PathBuf, base: u64, len: u64, magic: &[u8]) -> Option<Trailer> {
  let mut file = std::fs::File::open(&exe).ok()?;
  if base.checked_add(len)? > file.metadata().ok()?.len() {
    return None;
  }
  let tail_len = (8 + 4 + magic.len()) as u64; // table offset + entry count + magic
  if len < tail_len {
    return None;
  }
  let tail = len - tail_len;
  let mut tail_bytes = vec![0u8; tail_len as usize];
  file.seek(SeekFrom::Start(base + tail)).ok()?;
  file.read_exact(&mut tail_bytes).ok()?;
  if &tail_bytes[12..] != magic {
    return None;
  }
  let table_offset = u64::from_le_bytes(tail_bytes[0..8].try_into().ok()?);
  let count = u32::from_le_bytes(tail_bytes[8..12].try_into().ok()?);
  if table_offset >= tail || count == 0 {
    return None;
  }
  let mut table = vec![0u8; (tail - table_offset) as usize];
  file.seek(SeekFrom::Start(base + table_offset)).ok()?;
  file.read_exact(&mut table).ok()?;
  let mut cursor = 0usize;
  let mut sections = Vec::with_capacity(count as usize);
  for _ in 0..count {
    if cursor + 22 > table.len() {
      return None;
    }
    let kind = u32::from_le_bytes(table[cursor..cursor + 4].try_into().ok()?);
    let offset = u64::from_le_bytes(table[cursor + 4..cursor + 12].try_into().ok()?);
    let len = u64::from_le_bytes(table[cursor + 12..cursor + 20].try_into().ok()?);
    let name_len = u16::from_le_bytes(table[cursor + 20..cursor + 22].try_into().ok()?) as usize;
    cursor += 22;
    if cursor + name_len > table.len() {
      return None;
    }
    let name = std::str::from_utf8(&table[cursor..cursor + name_len]).ok()?.to_string();
    cursor += name_len;
    // Sections precede the table (bounds in payload-relative coordinates).
    if offset.checked_add(len)? > table_offset {
      return None;
    }
    sections.push(Section { kind, name, offset: base + offset, len });
  }
  // The entries must consume the table region exactly.
  if cursor != table.len() {
    return None;
  }
  Some(Trailer { exe, sections })
}

impl Trailer {
  /// The file-section index in the shape `fs::AssetsBase::Packed` mounts:
  /// name -> (absolute offset, length), kind-2 sections with names only.
  pub fn file_index(&self) -> HashMap<String, (u64, u64)> {
    self
      .sections
      .iter()
      .filter(|s| s.kind == SECTION_FILE && !s.name.is_empty())
      .map(|s| (s.name.clone(), (s.offset, s.len)))
      .collect()
  }

  /// Read one section's bytes (a ranged read into the image).
  pub fn section_bytes(&self, section: &Section) -> Result<Vec<u8>, String> {
    self.read_range(section.offset, section.len)
  }

  /// Read `len` bytes at absolute offset `offset` in the image.
  pub fn read_range(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    let err = |e| format!("read {}: {e}", self.exe.display());
    let mut file = std::fs::File::open(&self.exe).map_err(err)?;
    file.seek(SeekFrom::Start(offset)).map_err(err)?;
    let mut buf = vec![0u8; usize::try_from(len).map_err(|_| format!("read {}: too large", self.exe.display()))?];
    file.read_exact(&mut buf).map_err(err)?;
    Ok(buf)
  }
}
