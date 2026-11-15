//! Availability: whether a platform facility can be used here, as one of
//! three states, the same contract for every facility.
//!
//! - `Available`: the platform says yes, and a call is expected to work.
//! - `Unavailable`: a call cannot work here. It rejects with the
//!   facility's own `Unavailable` error (`DialogError::Unavailable`,
//!   `LinkError::Unavailable`, ...).
//! - `Unknown`: not settled yet (a probe is in flight; it ends in one of
//!   the other two), or only a call can tell (no probe reaches the answer
//!   without doing the work). Each facility's module doc says which.
//!
//! A call corrects the fact. When a call fails because the facility is
//! absent, its session records `Unavailable` before the call answers,
//! whatever the state was; a call that works on an `Unknown` facility
//! records `Available`. The second call never repeats the first call's
//! surprise. The app rule that goes with it: offer the feature unless it
//! is unavailable, and handle the facility's `Unavailable` error. So
//! `Unknown` never hides a feature that would have worked.
//!
//! Who sets what:
//! - Link, share, the clipboard image and haptics are probed once, at the
//!   first read (`of`) or the first `set`, whichever comes first. The first
//!   read, not `Platform::init`: headless flux opens links and never builds
//!   a `Platform`. No reader sees these before their probe, so the probe
//!   queues no change.
//! - Dialog is probed when the window opens (`window::open`), the first
//!   time a surface to attach a dialog to exists; it reads `Unknown` until
//!   then.
//! - Camera, microphone and audio output are set from their device lists
//!   (`Availability::of_devices`) when their SDL subsystem comes up, and
//!   re-read on SDL's device added and removed events. Both subsystems
//!   start lazily, at the first list or open (`camera::start`,
//!   `audio.rs`'s init), so the three read `Unknown` until an app uses
//!   them: the "only a call can tell" case.
//!
//! A change reaches the pump as `PlatformEvent::Availability`: `set`
//! records the new state, then queues the change, then wakes the pump
//! through the waker `Platform::init` stored here (`install_waker`). The
//! watch poll of the next wait drains the queue. So a reader woken by the
//! event reads the new state with `of`. With no waker stored the change
//! still queues and the next poll delivers it; with no pump at all
//! (headless flux), nothing drains the queue, and `of` still answers.
//! Changes are rare (a probe settling, a call correcting the fact), so an
//! undrained queue stays small.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, Once, PoisonError};

use crate::platform::Waker;

/// What a facility's availability is (see the module doc).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Availability {
  /// A call is expected to work.
  Available,
  /// A call cannot work here; it rejects with the facility's `Unavailable`.
  Unavailable,
  /// Not settled yet, or only a call can tell.
  #[default]
  Unknown,
}

impl Availability {
  /// The state's name: "available", "unavailable" or "unknown".
  pub fn name(self) -> &'static str {
    match self {
      Availability::Available => "available",
      Availability::Unavailable => "unavailable",
      Availability::Unknown => "unknown",
    }
  }

  /// The availability a device list of `count` devices shows: `Available`
  /// with one or more, `Unavailable` with none. The camera, the microphone
  /// and the audio output read their lists through this.
  pub(crate) fn of_devices(count: usize) -> Availability {
    if count == 0 {
      Availability::Unavailable
    } else {
      Availability::Available
    }
  }

  fn to_u8(self) -> u8 {
    match self {
      Availability::Available => STATE_AVAILABLE,
      Availability::Unavailable => STATE_UNAVAILABLE,
      Availability::Unknown => STATE_UNKNOWN,
    }
  }

  fn from_u8(raw: u8) -> Availability {
    match raw {
      STATE_AVAILABLE => Availability::Available,
      STATE_UNAVAILABLE => Availability::Unavailable,
      _ => Availability::Unknown,
    }
  }
}

// The states as the table stores them.
const STATE_UNKNOWN: u8 = 0;
const STATE_AVAILABLE: u8 = 1;
const STATE_UNAVAILABLE: u8 = 2;

/// A platform facility whose availability ore reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Facility {
  /// Opening a link (`crate::link`).
  Link,
  /// The file dialogs (`crate::dialog`).
  Dialog,
  /// The share sheet (`crate::share`).
  Share,
  /// An image on the clipboard (`crate::clipboard`).
  ClipboardImage,
  /// The haptic actuator (`crate::haptics`).
  Haptics,
  /// A camera device.
  Camera,
  /// A microphone (an audio recording device).
  Microphone,
  /// An audio output (a playback device).
  AudioOutput,
}

impl Facility {
  /// Every facility, in table order.
  pub const ALL: &[Facility] = &[
    Facility::Link,
    Facility::Dialog,
    Facility::Share,
    Facility::ClipboardImage,
    Facility::Haptics,
    Facility::Camera,
    Facility::Microphone,
    Facility::AudioOutput,
  ];

  /// The facility's name: "link", "dialog", "share", "clipboard-image",
  /// "haptics", "camera", "microphone" or "audio".
  pub fn name(self) -> &'static str {
    match self {
      Facility::Link => "link",
      Facility::Dialog => "dialog",
      Facility::Share => "share",
      Facility::ClipboardImage => "clipboard-image",
      Facility::Haptics => "haptics",
      Facility::Camera => "camera",
      Facility::Microphone => "microphone",
      Facility::AudioOutput => "audio",
    }
  }

  // The facility's slot in the table.
  fn index(self) -> usize {
    self as usize
  }
}

// One state per facility, indexed by `Facility::index`; Unknown at start.
static STATES: [AtomicU8; Facility::ALL.len()] = [const { AtomicU8::new(STATE_UNKNOWN) }; Facility::ALL.len()];

// The changes not yet drained by the pump, oldest first. Also the lock a
// write holds, so the queue's order is the order of the writes.
static CHANGES: Mutex<Vec<(Facility, Availability)>> = Mutex::new(Vec::new());

// The pump's waker, stored by `Platform::init`.
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

// The probes of the facilities that need no window, run once.
static PROBED: Once = Once::new();

/// The availability of `facility` now (see the module doc). Any thread.
pub fn of(facility: Facility) -> Availability {
  probe_once();
  Availability::from_u8(STATES[facility.index()].load(Ordering::Acquire))
}

/// Record `state` for `facility`. On a change, queue it for the pump and
/// wake the pump; the state is written before the change is queued, so a
/// reader woken by the event reads the new state. A repeat of the current
/// state queues nothing. Any thread.
pub(crate) fn set(facility: Facility, state: Availability) {
  probe_once();
  let changed = {
    let mut changes = CHANGES.lock().unwrap_or_else(PoisonError::into_inner);
    let previous = STATES[facility.index()].swap(state.to_u8(), Ordering::AcqRel);
    let changed = previous != state.to_u8();
    if changed {
      changes.push((facility, state));
    }
    changed
  };
  if changed {
    if let Some(waker) = WAKER.lock().unwrap_or_else(PoisonError::into_inner).as_ref() {
      waker.wake();
    }
  }
}

/// Store the pump's waker, so a change made while the pump sleeps wakes
/// it. Called once by `Platform::init`.
pub(crate) fn install_waker(waker: Waker) {
  *WAKER.lock().unwrap_or_else(PoisonError::into_inner) = Some(waker);
}

/// The changes queued since the last drain, oldest first. The pump's
/// watch poll and the tests.
#[cfg_attr(not(any(feature = "window", test)), allow(dead_code))]
pub(crate) fn drain() -> Vec<(Facility, Availability)> {
  std::mem::take(&mut *CHANGES.lock().unwrap_or_else(PoisonError::into_inner))
}

// The first answer of every facility that needs no window, stored without
// a change event: no reader sees a state before this ran (`of` and `set`
// run it first).
fn probe_once() {
  PROBED.call_once(|| {
    for (facility, state) in [
      (Facility::Link, crate::link::probe()),
      (Facility::Share, crate::share::probe()),
      (Facility::ClipboardImage, crate::clipboard::probe_image()),
      (Facility::Haptics, crate::haptics::probe()),
    ] {
      STATES[facility.index()].store(state.to_u8(), Ordering::Release);
    }
  });
}
