use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::{Converter, Playback};

/// The source rate of the converter test, Hz.
const SRC_RATE: u32 = 44_100;
/// The destination rate of the converter test, Hz.
const DST_RATE: u32 = 48_000;
/// The clip is this fraction of a second long (0.1 s).
const CLIPS_PER_SECOND: u32 = 10;
/// Frames the resampler may hold back or round off after a flush.
const CONVERTED_FRAMES_TOLERANCE: usize = 16;
/// How long the playback test waits for the dummy device's first pull.
const FIRST_PULL_TIMEOUT: Duration = Duration::from_secs(2);
/// The step of that wait.
const PULL_POLL_STEP: Duration = Duration::from_millis(10);

#[test]
fn converter_resamples_and_upmixes() {
  let input = vec![0.25_f32; (SRC_RATE / CLIPS_PER_SECOND) as usize];
  let expected = (DST_RATE / CLIPS_PER_SECOND) as usize;
  let mut converter = Converter::new((SRC_RATE, 1), (DST_RATE, 2)).expect("converter is created");
  converter.put(&input);
  converter.flush();
  let available = converter.available();
  assert!(available.abs_diff(expected) <= CONVERTED_FRAMES_TOLERANCE, "{available} frames, expected about {expected}");
  let mut out = vec![0.0_f32; available * 2];
  assert_eq!(converter.get(&mut out), available);
  converter.put(&input);
  converter.clear();
  assert_eq!(converter.available(), 0);
}

// SDL reads SDL_AUDIO_DRIVER when the audio subsystem is first initialized,
// so this test must run in a process where nothing opened audio before;
// ore's other tests do not.
#[test]
fn playback_pulls_on_the_dummy_driver() {
  sdl3::hint::set("SDL_AUDIO_DRIVER", "dummy");
  let pulls = Arc::new(AtomicU64::new(0));
  let counted = Arc::clone(&pulls);
  let opened = Playback::open(2, move |pull| {
    counted.fetch_add(1, Ordering::AcqRel);
    pull.put(&vec![0.0_f32; pull.frames() * 2]);
  });
  let playback = match opened {
    Ok(playback) => playback,
    Err(e) => {
      println!("Skipped: the dummy audio driver did not open: {e}");
      return;
    }
  };
  assert!(playback.device_rate() > 0);
  let started = Instant::now();
  while pulls.load(Ordering::Acquire) == 0 && started.elapsed() < FIRST_PULL_TIMEOUT {
    std::thread::sleep(PULL_POLL_STEP);
  }
  assert!(pulls.load(Ordering::Acquire) > 0, "no pull within {FIRST_PULL_TIMEOUT:?}");
}
