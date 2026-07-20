use crate::fs::FileWindow;
use std::io::{Read, Seek, SeekFrom, Write};

// A temp file with known content, plus a window over its middle section.
fn window(tag: &str) -> FileWindow {
  let path = std::env::temp_dir().join(format!("forge-fs-window-{}-{tag}", std::process::id()));
  let mut f = std::fs::File::create(&path).expect("create temp file");
  f.write_all(b"HEAD0123456789TAIL").expect("write temp file");
  // The temp file stays behind (deleting an open file is unix-only behavior);
  // create() truncates it on the next run.
  let file = std::fs::File::open(&path).expect("open temp file");
  FileWindow { file, start: 4, len: 10, pos: 0 }
}

#[test]
fn window_reads_are_clamped() {
  let mut w = window("read");
  let mut all = Vec::new();
  w.read_to_end(&mut all).expect("read to end");
  // Only the windowed bytes, never the surrounding HEAD/TAIL.
  assert_eq!(all, b"0123456789");
  // At end-of-window reads return 0, like end-of-file.
  let mut buf = [0u8; 4];
  assert_eq!(w.read(&mut buf).expect("read at end"), 0);
}

#[test]
fn window_seeks_are_window_relative() {
  let mut w = window("seek");
  let mut buf = [0u8; 3];

  assert_eq!(w.seek(SeekFrom::Start(7)).expect("seek start"), 7);
  w.read_exact(&mut buf).expect("read after seek");
  assert_eq!(&buf, b"789");

  assert_eq!(w.seek(SeekFrom::End(-2)).expect("seek end"), 8);
  let mut two = [0u8; 2];
  w.read_exact(&mut two).expect("read tail");
  assert_eq!(&two, b"89");

  assert_eq!(w.seek(SeekFrom::Current(-4)).expect("seek current"), 6);
  w.read_exact(&mut buf).expect("read after relative seek");
  assert_eq!(&buf, b"678");

  // Past-end seeks are allowed (reads return 0); before-start seeks error.
  assert_eq!(w.seek(SeekFrom::Start(99)).expect("seek past end"), 99);
  assert_eq!(w.read(&mut buf).expect("read past end"), 0);
  assert!(w.seek(SeekFrom::End(-11)).is_err());
}
