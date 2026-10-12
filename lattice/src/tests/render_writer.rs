// The frame writers of a render (render_host.rs), without an engine: frames
// go in through the bounded queue, PNGs come out under the prefix.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::render_host::{Frame, FrameWriters};

/// How long a test waits for the writers to report a failed write; they
/// take a 2x2 frame in well under a millisecond.
const FAILURE_WAIT: Duration = Duration::from_secs(5);

fn temp_dir(tag: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("sol-render-writer-test-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// A 2x2 frame whose pixels spell its index, so a file can be checked
/// against the frame it holds.
fn frame(index: u64) -> Frame {
  let v = index as u8;
  Frame { index, width: 2, height: 2, pixels: vec![v, 0, 0, 255, 0, v, 0, 255, 0, 0, v, 255, v, v, v, 255] }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
  tokio::runtime::Builder::new_current_thread().build().expect("tokio runtime").block_on(future)
}

#[test]
fn writes_every_frame_under_the_prefix() {
  let dir = temp_dir("all");
  std::fs::create_dir_all(&dir).expect("create the output directory");
  let prefix = dir.join("frame");
  let writers = FrameWriters::start(&prefix.to_string_lossy()).expect("the writers start");
  // More frames than the queue holds, so the driver side waits on it.
  let count = 12;
  block_on(async {
    for index in 0..count {
      assert!(writers.send(frame(index)).await, "the queue is open");
    }
  });
  assert_eq!(writers.finish().expect("every frame is written"), count);
  for index in 0..count {
    let path = dir.join(format!("frame-{index:06}.png"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{} is missing: {e}", path.display()));
    let decoded = forge::image::decode(&bytes, false).expect("a PNG");
    assert_eq!((decoded.width, decoded.height), (2, 2));
    assert_eq!(decoded.data, frame(index).pixels, "{} holds frame {index}", path.display());
  }
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn creates_the_output_directory() {
  let dir = temp_dir("mkdir");
  let prefix = dir.join("deep").join("er").join("f");
  let writers = FrameWriters::start(&prefix.to_string_lossy()).expect("the writers start");
  block_on(async {
    assert!(writers.send(frame(0)).await);
  });
  assert_eq!(writers.finish().expect("the frame is written"), 1);
  assert!(dir.join("deep").join("er").join("f-000000.png").is_file());
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_a_prefix_whose_directory_cannot_be_created() {
  let dir = temp_dir("blocked");
  std::fs::create_dir_all(&dir).expect("create the output directory");
  // A file where the prefix's directory would go.
  std::fs::write(dir.join("taken"), b"").expect("write the blocker");
  let prefix = dir.join("taken").join("f");
  let error = FrameWriters::start(&prefix.to_string_lossy()).err().expect("the writers refuse");
  assert!(error.contains("could not create"), "{error}");
  let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_write_is_reported_and_stops_the_driver() {
  let dir = temp_dir("fail");
  std::fs::create_dir_all(&dir).expect("create the output directory");
  // Frame 3's file name is taken by a directory, so its write fails while
  // the other frames go through.
  std::fs::create_dir(dir.join("frame-000003.png")).expect("create the blocker");
  let prefix = dir.join("frame");
  let writers = FrameWriters::start(&prefix.to_string_lossy()).expect("the writers start");
  block_on(async {
    for index in 0..4 {
      assert!(writers.send(frame(index)).await, "the queue stays open while a writer remains");
    }
  });
  // The driver reads the failure before its next frame.
  let deadline = Instant::now() + FAILURE_WAIT;
  while writers.failed().is_none() {
    assert!(Instant::now() < deadline, "the failed write is never reported");
    std::thread::yield_now();
  }
  assert!(writers.failed().expect("the failure is kept").contains("frame-000003.png"));
  let error = writers.finish().err().expect("the failure wins");
  assert!(error.contains("could not write"), "{error}");
  assert!(error.contains("frame-000003.png"), "{error}");
  for index in [0, 1, 2] {
    assert!(dir.join(format!("frame-{index:06}.png")).is_file(), "frame {index} is written");
  }
  let _ = std::fs::remove_dir_all(&dir);
}
