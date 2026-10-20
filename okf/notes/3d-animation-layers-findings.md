---
title: What the animation rig found
description: Facts that hold without the animation-layers plan - the Gears of War inertialization quintic front-loads its decay and needs the overshoot cap, a marker window is a state a seek must close and open, a binding's mask-share cache must refresh before the root-motion pass, a core call that adds motion must request a frame, and a sync group's leader must be copied out before its followers are written.
created: 2026-10-20
---

# What the animation rig found

Cut from [3d-animation-layers](../done/3d-animation-layers.md) at its close.

- The inertialization quintic of the Gears of War talk (x0, v0 matched
  at 0; zero offset, velocity and acceleration at t1; a0 = (-8 v0 t1 -
  20 x0) / t1^2) is NOT the smoothstep-like curve with zero initial
  acceleration: with v0 = 0 it keeps less than a fifth of the offset at
  the half-way mark (x'(t) = 20 t (t - 1)^3 for x0 = 1, t1 = 1), front-
  loading the decay. The cap t1 <= 5 x0 / |v0| when the velocity points
  at zero is what keeps it from crossing zero; a test that pins x(t1/2)
  at x0/2 is pinning the wrong curve.
- A window is a state, an instant is an event: a seek that jumps out of
  a window must END it (and one that lands in a window must BEGIN it),
  or every Begin an app pairs with its End (a hitbox, a cancel window)
  leaks. "A seek never fires" is true of instants only.
- The mask shares cached on a binding must be refreshed at the START of
  the rig's advance, before the root-motion pass reads them, not in the
  blend loop: a setMask followed by one advance otherwise weighted that
  frame's travel by the stale share (the test for root travel through a
  layer mask caught it).
- A core call that adds a player or a rig must request a frame: an app
  at rest (the test harness, a paused screen) never advanced it
  otherwise, and the pose stayed at rest with no error anywhere. The
  old create_player had the same hole, hidden because every play in
  the probes happened inside a mounting scene.
- Followers of a sync group cannot take the leader's borrowed Player
  while the group loop also walks the rig's players mutably: the
  leader's facts (window, phase, finished, marker sequence) are copied
  out first, then the followers are written. Rust's borrow checker
  found the aliasing the design had glossed over.
- Events a JS write causes itself (a seek's window begin/end, a
  destroy's window end) cannot ride the next frame's event delivery:
  the facade tears the player's handler down synchronously, so the
  frame-later event finds nobody. The core returns them from the call
  (`set_player` -> the window changes, `destroy_player` -> the windows
  closed) and the facade reports them at once, which is also the order
  an app expects: its own call's consequences before its next line.
- A single position track is a clip's "root" by the mixer's rule
  (topmost node any position channel targets) and a drifting one plays
  in place by default: a test clip that slides one joint is pinned
  unless it says `inPlace: false`. Real additive clips (a flinch) never
  translate the hips, so the rule does not bite them.
- A player fading out must blend ONCE MORE at weight zero before it
  drops, or the pose holds one frame short of its destination: removed
  the frame its weight crosses zero, a stop-with-fade left the joints at
  the last positive weight (a sixth of the way from rest, at 60 Hz and
  a 100 ms fade). The dying frame is a blend, not a step: no wrap,
  marker or travel comes from it.
- IK keeps no rest pose of its own: its "rest forward" and "rest bend"
  are the primary pose (what the players and the frame wrote), the
  input a solve starts from, not a declared rest. The springs did keep
  one (the rotation at their declaration, packed from JS), and the rig
  read a table the facade copied from the JS mirrors: both now read the
  node's declared rest (`set_rest`, a model's file transform, published
  at every scene enter), the pose as it stands when none is declared.
