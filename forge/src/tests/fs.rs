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

#[tokio::test]
async fn rename_moves_files_and_dirs() {
  let dir = std::env::temp_dir().join(format!("forge-fs-rename-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).expect("create test dir");
  let at = |name: &str| dir.join(name).to_string_lossy().into_owned();

  // A file moves: the source is gone, the contents arrive under the new name.
  crate::fs::write(&at("draft.txt"), b"hello").await.expect("write draft");
  crate::fs::rename(&at("draft.txt"), &at("final.txt")).await.expect("rename file");
  assert!(!crate::fs::file_exists(&at("draft.txt")).await);
  assert_eq!(crate::fs::read(&at("final.txt")).await.expect("read final"), b"hello");

  // A directory moves whole, with what is inside it.
  crate::fs::create_dir(&at("old/inner")).await.expect("create dir");
  crate::fs::write(&at("old/inner/note.txt"), b"kept").await.expect("write nested");
  crate::fs::rename(&at("old"), &at("new")).await.expect("rename dir");
  assert!(!crate::fs::dir_exists(&at("old")).await);
  assert_eq!(crate::fs::read(&at("new/inner/note.txt")).await.expect("read moved"), b"kept");

  // An existing target is replaced, not refused (what the OS rename does).
  crate::fs::write(&at("other.txt"), b"replacement").await.expect("write other");
  crate::fs::rename(&at("other.txt"), &at("final.txt")).await.expect("rename over target");
  assert_eq!(crate::fs::read(&at("final.txt")).await.expect("read replaced"), b"replacement");

  // A missing source is an error: nothing moved, unlike remove's missing path.
  let gone = crate::fs::rename(&at("missing.txt"), &at("wherever.txt")).await;
  assert!(gone.is_err(), "rename of a missing source should error");

  let _ = std::fs::remove_dir_all(&dir);
}

// Sequential appends land in call order and only resolve once the bytes are
// in the file. Each append opens its own O_APPEND handle; without a flush the
// background write of one append can race the next one's (seen as swapped
// chunks in a downloaded file on a slow device).
#[tokio::test]
async fn append_keeps_sequential_chunks_in_order() {
  // Enough chunks of varying sizes to give a race a fair chance; a fast
  // desktop may still not hit it, so this is a regression guard.
  const CHUNKS: usize = 400;
  let path = std::env::temp_dir().join(format!("forge-fs-append-{}", std::process::id()));
  let path = path.to_string_lossy().into_owned();
  let _ = std::fs::remove_file(&path);

  let mut expected = Vec::new();
  for i in 0..CHUNKS {
    let chunk: Vec<u8> = format!("chunk-{i:04}|").repeat(1 + i % 7).into_bytes();
    crate::fs::append(&path, &chunk).await.expect("append chunk");
    expected.extend_from_slice(&chunk);
  }
  let actual = crate::fs::read(&path).await.expect("read appended file");
  assert_eq!(actual.len(), expected.len());
  assert!(actual == expected, "appended chunks are out of order");
  let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn realpath_resolves_symlinks_and_dots() {
  let dir = std::env::temp_dir().join(format!("forge-fs-realpath-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("real")).expect("create real dir");
  // The reference is realpath's own spelling of the plain path: canonical,
  // but without the verbatim `\\?\` prefix std's canonicalize keeps on
  // Windows (realpath strips it so registries key on what the shell prints).
  let real = crate::fs::realpath(&dir.join("real").to_string_lossy()).await.expect("realpath of real dir");
  assert!(!real.starts_with(r"\\?\"), "verbatim prefix leaked: {real}");
  assert!(std::path::Path::new(&real).is_absolute());

  // ".." and "." collapse to the real directory.
  let dotted = dir.join("real").join("..").join(".").join("real");
  let resolved = crate::fs::realpath(&dotted.to_string_lossy()).await.expect("realpath of dotted path");
  assert_eq!(resolved, real);

  #[cfg(unix)]
  {
    std::os::unix::fs::symlink(&real, dir.join("link")).expect("create symlink");
    let via_link = crate::fs::realpath(&dir.join("link").to_string_lossy()).await.expect("realpath of symlink");
    assert_eq!(via_link, real);
  }

  let missing = crate::fs::realpath(&dir.join("missing").to_string_lossy()).await;
  assert!(missing.is_err(), "a missing path errors");
  let _ = std::fs::remove_dir_all(&dir);
}

// A directory watch reports a created file and the target name of a rename
// (an editor's atomic save), and refuses a directory that does not exist.
#[tokio::test]
async fn dir_watcher_reports_create_and_rename_target() {
  use crate::fs::{DirWatcher, WatchKind};
  use std::time::Duration;

  let dir = std::env::temp_dir().join(format!("forge-fs-watch-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(&dir).expect("create temp dir");
  let mut watcher = DirWatcher::open(&dir.to_string_lossy(), false).expect("open watcher");

  std::fs::write(dir.join("a.tmp"), "x").expect("write a.tmp");
  std::fs::rename(dir.join("a.tmp"), dir.join("a.txt")).expect("rename a.tmp");

  let mut seen = Vec::new();
  let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
  while tokio::time::Instant::now() < deadline {
    let next = tokio::time::timeout_at(deadline, watcher.recv()).await;
    let Ok(Some(event)) = next else { break };
    seen.push(event);
    if seen.iter().any(|e| e.kind == WatchKind::Rename && e.path.ends_with("a.txt")) {
      break;
    }
  }
  assert!(seen.iter().any(|e| e.kind == WatchKind::Create && e.path.ends_with("a.tmp")), "no create: {seen:?}");
  assert!(seen.iter().any(|e| e.kind == WatchKind::Rename && e.path.ends_with("a.txt")), "no rename target: {seen:?}");

  let missing = dir.join("no-such-dir");
  assert!(DirWatcher::open(&missing.to_string_lossy(), false).is_err());
  let _ = std::fs::remove_dir_all(&dir);
}
