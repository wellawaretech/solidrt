// FrameRelease (vsync.rs): the vsync frame-release state machine, tested as
// pure decisions over an injected clock - the invariants the run loop used
// to hold in comments across four mutation sites.

use std::time::{Duration, Instant};

use crate::vsync::{FramePacing, FrameRelease, PacingChange, Release, Wake};

const PERIOD: Duration = Duration::from_millis(16);

fn emits(r: Release) -> bool {
  matches!(r, Release::Emit { .. })
}

fn deferred_arm(r: Release) -> Option<Duration> {
  match r {
    Release::Deferred { arm } => arm,
    Release::Emit { .. } => panic!("expected Deferred"),
  }
}

#[test]
fn no_backend_always_emits() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(false, t0);
  assert!(emits(fr.on_present(t0, PERIOD)));
  assert!(fr.idle());
  assert!(fr.wait_deadline().is_none());
}

#[test]
fn swap_paced_emits_directly() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  // Nothing was deferred, so the switch releases nothing (and the caller
  // must not touch the frame-signal clock).
  assert!(matches!(fr.set_pacing(FramePacing::SwapPaced), PacingChange::Changed { released: 0 }));
  assert!(emits(fr.on_present(t0, PERIOD)));
  // Re-sending the same policy is not a change (and must not re-log).
  assert!(matches!(fr.set_pacing(FramePacing::SwapPaced), PacingChange::Unchanged));
}

#[test]
fn first_present_out_of_idle_arms_the_chain() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  let arm = deferred_arm(fr.on_present(t0, PERIOD));
  assert!(arm.is_some(), "chain start must arm a vsync request");
  assert!(!fr.idle());
  // The fallback deadline sits past the next vsync (request + period +
  // delay + slack), so a healthy signal can always beat it.
  let deadline = fr.wait_deadline().expect("a deferred present has a deadline");
  assert!(deadline > t0 + PERIOD);
  // A second present while armed must not send a superseding request.
  assert!(deferred_arm(fr.on_present(t0, PERIOD)).is_none());
}

#[test]
fn signal_releases_all_pending_and_prearms() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  deferred_arm(fr.on_present(t0, PERIOD));
  match fr.on_wake(t0 + PERIOD, PERIOD, Some(t0 + PERIOD)) {
    Wake::Release { emit, timed_out, arm } => {
      assert_eq!(emit, 2);
      assert!(!timed_out);
      assert!(arm.is_some(), "the release pre-arms the next vsync");
    }
    Wake::Idle | Wake::Banked { .. } => panic!("a taken signal with pending presents must release"),
  }
  // Nothing is pending, but the frame this emission triggers is in flight
  // until its present returns: not idle (no Tick through the swap), and
  // the loop waits toward the armed deadline.
  assert!(!fr.idle());
  assert!(fr.wait_deadline().is_some());
}

#[test]
fn fallback_fires_only_at_the_deadline() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let deadline = fr.wait_deadline().expect("deferred");
  // Before the deadline with no signal: keep waiting.
  assert!(matches!(fr.on_wake(deadline - Duration::from_millis(1), PERIOD, None), Wake::Idle));
  // At the deadline: release with the timeout marked, and a fresh request
  // armed (superseding the late signal, which try_take will discard).
  match fr.on_wake(deadline, PERIOD, None) {
    Wake::Release { emit, timed_out, arm } => {
      assert_eq!(emit, 1);
      assert!(timed_out);
      assert!(arm.is_some());
    }
    Wake::Idle | Wake::Banked { .. } => panic!("the fallback must release at the deadline"),
  }
}

#[test]
fn signal_with_nothing_pending_ends_the_chain() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let t1 = t0 + PERIOD;
  assert!(matches!(fr.on_wake(t1, PERIOD, Some(t1)), Wake::Release { .. }));
  // Demand stops (the UI built nothing for the emission). The pre-armed
  // request's signal finds nothing pending with a frame nominally in
  // flight, so it is banked (one spare callback); the signal after it ends
  // the chain (a second spare) and idle ticks resume.
  assert!(matches!(fr.on_wake(t1 + PERIOD, PERIOD, Some(t1 + PERIOD)), Wake::Banked { .. }));
  assert!(!fr.idle());
  assert!(matches!(fr.on_wake(t1 + 2 * PERIOD, PERIOD, Some(t1 + 2 * PERIOD)), Wake::Idle));
  assert!(fr.idle(), "the chain ended: ticks may resume");
  assert!(fr.wait_deadline().is_none());
  // The next present out of idle starts a fresh chain.
  assert!(deferred_arm(fr.on_present(t1 + 3 * PERIOD, PERIOD)).is_some());
}

#[test]
fn leaving_vsync_locked_releases_deferred_presents() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  match fr.set_pacing(FramePacing::SwapPaced) {
    PacingChange::Changed { released } => assert_eq!(released, 1),
    PacingChange::Unchanged => panic!("switching to SwapPaced is a change"),
  }
  assert!(fr.idle());
  // Back to VsyncLocked: a change, but nothing to release.
  assert!(matches!(fr.set_pacing(FramePacing::VsyncLocked), PacingChange::Changed { released: 0 }));
}

#[test]
fn pacing_excursion_disarms_so_the_return_arms_fresh() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  // Leave VsyncLocked while a request is outstanding, then return before
  // its signal drains: the first present back must arm a fresh request
  // (superseding the old one) rather than trust the stale deadline.
  fr.set_pacing(FramePacing::SwapPaced);
  fr.set_pacing(FramePacing::VsyncLocked);
  assert!(deferred_arm(fr.on_present(t0 + PERIOD, PERIOD)).is_some());
  // The old request's signal, arriving late as a superseded generation,
  // never reaches the machine (VsyncSource::try_take discards it), so no
  // on_wake models it here.
}

// A frame is in flight (signal emitted, present not back) while the swap
// blocks; the vsync signal that beats the present must not end the chain.
#[test]
fn signal_ahead_of_the_in_flight_present_is_banked() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let t1 = t0 + PERIOD;
  assert!(matches!(fr.on_wake(t1, PERIOD, Some(t1)), Wake::Release { .. }));
  // The frame this emission triggers is in flight: not idle, and the loop
  // waits toward the armed deadline rather than ticking through the swap.
  assert!(!fr.idle(), "a frame in flight is not idle");
  assert!(fr.wait_deadline().is_some());
  // Its signal arrives first (the swap is still blocking): banked, the
  // request for the following vsync armed (the signal disarmed the last).
  let t2 = t1 + PERIOD;
  match fr.on_wake(t2, PERIOD, Some(t2)) {
    Wake::Banked { arm } => assert!(arm.is_some(), "the bank keeps the next vsync armed"),
    Wake::Release { .. } => panic!("nothing is pending to release"),
    Wake::Idle => panic!("a signal ahead of an in-flight present must be banked, not end the chain"),
  }
  assert!(!fr.idle());
  // The present returns: released at once, nothing to arm (already armed),
  // and referenced at the banked signal's vsync, not at the swap's return.
  match fr.on_present(t2 + Duration::from_millis(2), PERIOD) {
    Release::Emit { arm, vsync } => {
      assert!(arm.is_none());
      assert_eq!(vsync, Some(t2), "a banked release carries its vsync as the reference");
    }
    Release::Deferred { .. } => panic!("a banked signal releases the present on arrival"),
  }
  // Back to the normal chain: the next present defers to its signal.
  let t3 = t2 + PERIOD;
  assert!(deferred_arm(fr.on_present(t3, PERIOD)).is_none());
  assert!(matches!(fr.on_wake(t3 + Duration::from_millis(8), PERIOD, Some(t3 + Duration::from_millis(8))), Wake::Release { emit: 1, .. }));
}

// Neither present nor signal by the armed deadline (a dead vsync source
// under a stalled swap): the in-flight window is given up so the loop does
// not wait on it forever, and a present arriving later starts afresh.
#[test]
fn in_flight_window_is_given_up_at_the_deadline() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let t1 = t0 + PERIOD;
  assert!(matches!(fr.on_wake(t1, PERIOD, Some(t1)), Wake::Release { .. }));
  let deadline = fr.wait_deadline().expect("in flight waits toward the deadline");
  assert!(matches!(fr.on_wake(deadline - Duration::from_millis(1), PERIOD, None), Wake::Idle));
  assert!(!fr.idle());
  assert!(matches!(fr.on_wake(deadline, PERIOD, None), Wake::Idle));
  assert!(fr.idle(), "past the deadline the window is given up");
  assert!(fr.wait_deadline().is_none());
  assert!(deferred_arm(fr.on_present(deadline + PERIOD, PERIOD)).is_some());
}

// Leaving VsyncLocked forgets a banked signal with the rest of the vsync
// state: the present it was waiting for emits under the new policy.
#[test]
fn leaving_vsync_locked_forgets_the_bank() {
  let t0 = Instant::now();
  let mut fr = FrameRelease::new(true, t0);
  deferred_arm(fr.on_present(t0, PERIOD));
  let t1 = t0 + PERIOD;
  assert!(matches!(fr.on_wake(t1, PERIOD, Some(t1)), Wake::Release { .. }));
  assert!(matches!(fr.on_wake(t1 + PERIOD, PERIOD, Some(t1 + PERIOD)), Wake::Banked { .. }));
  assert!(matches!(fr.set_pacing(FramePacing::SwapPaced), PacingChange::Changed { released: 0 }));
  assert!(fr.idle());
  match fr.on_present(t1 + PERIOD + Duration::from_millis(2), PERIOD) {
    Release::Emit { arm, .. } => assert!(arm.is_none(), "SwapPaced never arms"),
    Release::Deferred { .. } => panic!("SwapPaced emits directly"),
  }
}
