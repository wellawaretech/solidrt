// Frame-signal coalescing in the event loop (see lib.rs, runtime.rs): a
// batch keeps only its newest frame signal, and that survivor must carry
// the refresh counts of the signals it superseded, or the app timeline
// loses every refresh a busy JS thread let pile up (measured: a 20 fps app
// whose tick ran at a third of wall time).

use crate::runtime::coalesce_frame_signals;
use alloy::AlloyEvent;

fn refreshes(e: &AlloyEvent) -> u32 {
  match e {
    AlloyEvent::FrameRendered { refreshes, .. } | AlloyEvent::Tick { refreshes, .. } => *refreshes,
    _ => panic!("not a frame signal"),
  }
}

#[test]
fn survivor_carries_the_superseded_counts() {
  let a = AlloyEvent::Tick { frame: 10, fps: 60, refreshes: 1 };
  let b = AlloyEvent::FrameRendered { frame: 10, fps: 60, refreshes: 0 };
  let c = AlloyEvent::Tick { frame: 11, fps: 60, refreshes: 2 };
  let folded = coalesce_frame_signals(coalesce_frame_signals(a, b), c);
  assert_eq!(refreshes(&folded), 3);
  assert!(matches!(folded, AlloyEvent::Tick { frame: 11, .. }), "the newest signal's identity survives");
}

#[test]
fn a_present_after_ticks_stays_a_present() {
  let ticks = coalesce_frame_signals(
    AlloyEvent::Tick { frame: 5, fps: 60, refreshes: 1 },
    AlloyEvent::Tick { frame: 5, fps: 60, refreshes: 1 },
  );
  let folded = coalesce_frame_signals(ticks, AlloyEvent::FrameRendered { frame: 5, fps: 60, refreshes: 1 });
  assert!(matches!(folded, AlloyEvent::FrameRendered { frame: 5, refreshes: 3, .. }));
}
