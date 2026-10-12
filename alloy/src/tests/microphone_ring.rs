//! The microphone sample ring on its own (see microphone/ring.rs): push,
//! read by cursor, several readers, the window and the dropped count, wait,
//! timeout, close waking waiters. No device needed: the tests play the
//! producer.

use std::time::Duration;

use crate::microphone::{MicWait, SampleRing, RING_SECONDS};

/// A wait long enough that a wake from the other thread always lands inside
/// it, short enough that a test which should not wait that long fails fast.
const WAIT: Duration = Duration::from_secs(5);

/// A wait a test expects to run out.
const SHORT: Duration = Duration::from_millis(30);

/// A rate that makes the window `RING_SECONDS` samples, so overrun tests
/// need a handful of samples.
const TINY_RATE: u32 = 1;

fn run(from: u32, to: u32) -> Vec<f32> {
  (from..to).map(|i| i as f32).collect()
}

#[test]
fn empty_until_the_first_push() {
  let ring = SampleRing::new(16_000, 7);
  let mut reader = ring.reader();
  assert_eq!((reader.sample_rate(), reader.opened_ns(), reader.available()), (16_000, 7, 0));
  assert!(matches!(reader.wait(1, SHORT), MicWait::Timeout));
  let mut got = Vec::new();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.dropped, read.count), (0, 0, 0));
  assert!(got.is_empty());
}

#[test]
fn a_read_drains_what_came_since_the_last() {
  let ring = SampleRing::new(16_000, 0);
  let mut reader = ring.reader();
  ring.push(&run(0, 5));
  let mut got = Vec::new();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.dropped, read.count), (0, 0, 5));
  assert_eq!(got, run(0, 5));
  ring.push(&run(5, 8));
  got.clear();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.count), (5, 3));
  assert_eq!(got, run(5, 8));
  assert_eq!(reader.available(), 0);
}

#[test]
fn every_reader_hears_every_sample() {
  let ring = SampleRing::new(16_000, 0);
  let mut first = ring.reader();
  let mut second = ring.reader();
  ring.push(&run(0, 4));
  let mut got = Vec::new();
  first.read_into(&mut got).expect("open");
  ring.push(&run(4, 6));
  let mut other = Vec::new();
  second.read_into(&mut other).expect("open");
  assert_eq!(got, run(0, 4));
  assert_eq!(other, run(0, 6));
  got.clear();
  first.read_into(&mut got).expect("open");
  assert_eq!(got, run(4, 6));
}

#[test]
fn a_new_reader_hears_from_now() {
  let ring = SampleRing::new(16_000, 0);
  ring.push(&run(0, 5));
  let mut reader = ring.reader();
  assert_eq!(reader.available(), 0);
  ring.push(&run(5, 7));
  let mut got = Vec::new();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.count), (5, 2));
  assert_eq!(got, run(5, 7));
}

#[test]
fn a_reader_behind_the_window_loses_the_oldest_and_is_told() {
  let ring = SampleRing::new(TINY_RATE, 0);
  let window = RING_SECONDS as usize;
  let mut reader = ring.reader();
  ring.push(&run(0, 25));
  assert_eq!(reader.available(), window);
  let mut got = Vec::new();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.dropped, read.count), (15, 15, window));
  assert_eq!(got, run(15, 25));
  // Caught up: the next read reports no loss.
  ring.push(&run(25, 27));
  got.clear();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.dropped, read.count), (25, 0, 2));
}

#[test]
fn the_window_wraps_in_pieces() {
  let ring = SampleRing::new(TINY_RATE, 0);
  let mut reader = ring.reader();
  // Three pushes that straddle the wrap point of a 10-sample window.
  ring.push(&run(0, 7));
  let mut got = Vec::new();
  reader.read_into(&mut got).expect("open");
  ring.push(&run(7, 12));
  ring.push(&run(12, 14));
  got.clear();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!((read.position, read.dropped, read.count), (7, 0, 7));
  assert_eq!(got, run(7, 14));
}

#[test]
fn push_with_fills_the_scratch_in_place() {
  let ring = SampleRing::new(16_000, 0);
  let mut reader = ring.reader();
  ring.push_with(4, |scratch| {
    assert_eq!(scratch.len(), 4);
    scratch[..3].copy_from_slice(&[1.0, 2.0, 3.0]);
    3
  });
  let mut got = Vec::new();
  let read = reader.read_into(&mut got).expect("open");
  assert_eq!(read.count, 3);
  assert_eq!(got, vec![1.0, 2.0, 3.0]);
}

#[test]
fn wait_wakes_on_a_push_from_another_thread() {
  let ring = SampleRing::new(16_000, 0);
  let reader = ring.reader();
  let producer = ring.clone();
  let pusher = std::thread::spawn(move || {
    std::thread::sleep(Duration::from_millis(20));
    producer.push(&run(0, 3));
  });
  assert!(matches!(reader.wait(3, WAIT), MicWait::Samples(3)));
  pusher.join().expect("pusher thread");
  // Fewer than asked for keeps waiting until the deadline.
  assert!(matches!(reader.wait(4, SHORT), MicWait::Timeout));
}

#[test]
fn close_wakes_waiters_and_ends_reads() {
  let ring = SampleRing::new(16_000, 0);
  let mut reader = ring.reader();
  ring.push(&run(0, 2));
  let closer = ring.clone();
  let waiter = std::thread::spawn(move || {
    std::thread::sleep(Duration::from_millis(20));
    closer.close();
  });
  assert!(matches!(reader.wait(10, WAIT), MicWait::Closed));
  waiter.join().expect("closer thread");
  assert!(reader.closed());
  assert_eq!(reader.available(), 0);
  let mut got = Vec::new();
  assert!(reader.read_into(&mut got).is_err());
  // A push after the close is dropped, and a reader made after it is closed too.
  ring.push(&run(2, 4));
  assert!(ring.reader().closed());
}
