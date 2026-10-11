//! The camera frame slot on its own (see camera/tap.rs): publish, latest,
//! wait for a newer frame, timeout, close waking waiters, buffer reuse. No
//! camera needed: the tests play the producer.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::camera::{FrameTap, TapWait};

/// A wait long enough that a wake from the other thread always lands inside
/// it, short enough that a test which should not wait that long fails fast.
const WAIT: Duration = Duration::from_secs(5);

/// A wait a test expects to run out.
const SHORT: Duration = Duration::from_millis(30);

fn frame_of(byte: u8) -> Vec<u8> {
  vec![byte; 2 * 2 * 4]
}

#[test]
fn empty_until_the_first_frame() {
  let tap = FrameTap::new();
  assert!(tap.latest().is_none());
  assert!(matches!(tap.wait_newer(0, SHORT), TapWait::Timeout));
  tap.publish(7, 2, 2, frame_of(1));
  let latest = tap.latest().expect("a frame after the publish");
  assert_eq!((latest.seq, latest.timestamp_ns, latest.width, latest.height), (1, 7, 2, 2));
  assert_eq!(latest.pixels, frame_of(1));
}

#[test]
fn seq_counts_every_publish_and_latest_wins() {
  let tap = FrameTap::new();
  for byte in 1..=3 {
    tap.publish(0, 2, 2, frame_of(byte));
  }
  let latest = tap.latest().expect("a frame");
  assert_eq!(latest.seq, 3);
  assert_eq!(latest.pixels, frame_of(3));
}

#[test]
fn wait_newer_returns_at_once_when_a_newer_frame_exists() {
  let tap = FrameTap::new();
  tap.publish(0, 2, 2, frame_of(1));
  tap.publish(0, 2, 2, frame_of(2));
  let started = Instant::now();
  match tap.wait_newer(1, WAIT) {
    TapWait::Frame(frame) => assert_eq!(frame.seq, 2),
    _ => panic!("expected the newer frame"),
  }
  assert!(started.elapsed() < WAIT);
}

#[test]
fn wait_newer_times_out_without_a_newer_frame() {
  let tap = FrameTap::new();
  tap.publish(0, 2, 2, frame_of(1));
  let started = Instant::now();
  assert!(matches!(tap.wait_newer(1, SHORT), TapWait::Timeout));
  assert!(started.elapsed() >= SHORT);
}

#[test]
fn a_publish_wakes_a_waiter() {
  let tap = FrameTap::new();
  let waiter = {
    let tap = tap.clone();
    std::thread::spawn(move || tap.wait_newer(0, WAIT))
  };
  // Let the waiter block before the publish; a publish that lands first is
  // still seen, so this only makes the wake path the one exercised.
  std::thread::sleep(SHORT);
  tap.publish(42, 2, 2, frame_of(9));
  match waiter.join().expect("waiter thread") {
    TapWait::Frame(frame) => assert_eq!((frame.seq, frame.timestamp_ns), (1, 42)),
    _ => panic!("expected the published frame"),
  }
}

#[test]
fn close_wakes_waiters_and_answers_closed_from_then_on() {
  let tap = FrameTap::new();
  tap.publish(0, 2, 2, frame_of(1));
  let waiter = {
    let tap = tap.clone();
    std::thread::spawn(move || tap.wait_newer(1, WAIT))
  };
  std::thread::sleep(SHORT);
  tap.close();
  assert!(matches!(waiter.join().expect("waiter thread"), TapWait::Closed));
  assert!(tap.closed());
  assert!(tap.latest().is_none());
  assert!(matches!(tap.wait_newer(0, SHORT), TapWait::Closed));
  // A late publish from a producer that has not noticed yet is dropped.
  tap.publish(0, 2, 2, frame_of(2));
  assert!(tap.latest().is_none());
}

#[test]
fn the_previous_buffer_is_reused_when_no_consumer_holds_it() {
  let tap = FrameTap::new();
  assert!(tap.take_spare().is_empty(), "nothing to reuse before the first frame");
  let first = frame_of(1);
  let first_ptr = first.as_ptr();
  tap.publish(0, 2, 2, first);
  // The first frame is held only by the slot, so the second publish frees
  // its buffer for the third.
  tap.publish(0, 2, 2, frame_of(2));
  let spare = tap.take_spare();
  assert_eq!(spare.as_ptr(), first_ptr, "the unheld frame's buffer comes back as the spare");
  assert_eq!(spare.len(), 2 * 2 * 4);
  assert!(tap.take_spare().is_empty(), "a spare is handed out once");
}

#[test]
fn a_held_frame_keeps_its_buffer() {
  let tap = FrameTap::new();
  tap.publish(0, 2, 2, frame_of(1));
  let held: Arc<_> = tap.latest().expect("a frame");
  tap.publish(0, 2, 2, frame_of(2));
  assert!(tap.take_spare().is_empty(), "a frame a consumer holds is not recycled");
  assert_eq!(held.pixels, frame_of(1), "the consumer's frame is intact");
  drop(held);
  // Dropped after the publish that would have recycled it: freed, not
  // recycled, and the next publish recycles the second frame instead.
  tap.publish(0, 2, 2, frame_of(3));
  assert_eq!(tap.take_spare(), frame_of(2));
}

#[test]
fn two_unheld_frames_come_back_as_two_spares_and_no_more() {
  let tap = FrameTap::new();
  for byte in 1..=4 {
    tap.publish(0, 2, 2, frame_of(byte));
  }
  // Frames 1, 2 and 3 were replaced unheld; the pool keeps two of them.
  assert!(!tap.take_spare().is_empty());
  assert!(!tap.take_spare().is_empty());
  assert!(tap.take_spare().is_empty(), "the pool holds two buffers, not every replaced frame");
}
