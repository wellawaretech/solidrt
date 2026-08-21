use crate::trailer::{self, SECTION_FILE, SECTION_MANIFEST};
use std::io::Write;

const MAGIC: &[u8] = b"TEST\x88\x44";

// Build a trailer image the way packages/cli/src/packer.ts packSections does:
// a fake runner prefix, section bytes, table, tail.
fn image(sections: &[(u32, &str, &[u8])]) -> Vec<u8> {
  let mut out = b"RUNNER".to_vec();
  let mut entries: Vec<u8> = Vec::new();
  for &(kind, name, bytes) in sections {
    entries.extend_from_slice(&kind.to_le_bytes());
    entries.extend_from_slice(&(out.len() as u64).to_le_bytes());
    entries.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    entries.extend_from_slice(&(name.len() as u16).to_le_bytes());
    entries.extend_from_slice(name.as_bytes());
    out.extend_from_slice(bytes);
  }
  let table_offset = out.len() as u64;
  out.extend_from_slice(&entries);
  out.extend_from_slice(&table_offset.to_le_bytes());
  out.extend_from_slice(&(sections.len() as u32).to_le_bytes());
  out.extend_from_slice(MAGIC);
  out
}

fn write_image(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
  let path = std::env::temp_dir().join(format!("forge-trailer-{}-{tag}", std::process::id()));
  let mut f = std::fs::File::create(&path).expect("create temp file");
  f.write_all(bytes).expect("write temp file");
  path
}

#[test]
fn round_trip() {
  let bytes = image(&[
    (SECTION_MANIFEST, "", b"{\"app\":true}"),
    (SECTION_FILE, "bundle.bin", b"MAIN"),
    (SECTION_FILE, "isolates/worker.bin", b"WORKER"),
  ]);
  let path = write_image("roundtrip", &bytes);
  let trailer = trailer::read(path, MAGIC).expect("parse trailer");
  assert_eq!(trailer.sections.len(), 3);
  let manifest = &trailer.sections[0];
  assert_eq!(manifest.kind, SECTION_MANIFEST);
  assert_eq!(trailer.section_bytes(manifest).expect("read manifest"), b"{\"app\":true}");
  let index = trailer.file_index();
  assert_eq!(index.len(), 2);
  let &(offset, len) = index.get("isolates/worker.bin").expect("indexed isolate");
  assert_eq!(trailer.read_range(offset, len).expect("read isolate"), b"WORKER");
}

#[test]
fn rejects_bad_images() {
  // No trailer at all.
  let plain = write_image("plain", b"RUNNER ONLY");
  assert!(trailer::read(plain, MAGIC).is_none());
  // Wrong magic.
  let bytes = image(&[(SECTION_FILE, "bundle.bin", b"MAIN")]);
  let wrong = write_image("wrong-magic", &bytes);
  assert!(trailer::read(wrong, b"OTHER\x88\x44").is_none());
  // A section reaching past the table start.
  let mut overrun = image(&[(SECTION_FILE, "bundle.bin", b"MAIN")]);
  let entry_len_at = overrun.len() - MAGIC.len() - 12 - ("bundle.bin".len() + 2) - 8;
  overrun[entry_len_at..entry_len_at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
  let overrun = write_image("overrun", &overrun);
  assert!(trailer::read(overrun, MAGIC).is_none());
}
