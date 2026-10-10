---
title: Pause and time-scale the spatial core's motion
description: An app cannot pause the motion the spatial core runs - node transitions with their delays, enters, exits and stagger, morph weight tracks, clip players - because all of it runs on the one app clock, so a pause screen still lets it finish; the only freeze is the dev control API's whole-app clock, which stops the pause screen too.
created: 2026-10-10
completed: 2026-10-10
---

# Pause and time-scale the spatial core's motion

## Symptom

An app adds a pause: it stops its own `onFrame` logic, but every native
transition already in flight runs to its end, held (`delay`) writes
still apply, enters and exits play out, and clips keep playing. There is
nothing to call.

What existed:

- Node transitions: no pause. `NodeTransitions`
  (alloy/src/spatial/transitions.rs) held one `now_ms`, stamped with
  app time every frame (`spatial::stamp_clock`), and every track, held
  write and staggered exit was timed on it in absolute clock ms.
- Clip players: the core took a speed change after start
  (`setPlayer(player, { speed })`), but the 3d mixer only takes `speed`
  at `play()`, so a playing clip could not be paused without restarting.
- 2d sprite animations (`pause()`) and video (`pause()`) can pause.
- The dev control API's `/clock?scale=0` freezes app time for the whole
  app: dev tooling, and it freezes the pause screen with the world.

What an app needs is a scoped pause: the world stops, the pause screen
keeps animating. Often a scale too (slow motion), with 0 as paused.

## Done looks like

- A `timeScale` on a spatial node scales the clock of the native motion
  in its subtree: node transitions (running tracks, held writes, enters,
  exits, stagger offsets), weight tracks, and clip players and root
  motion clocked on its nodes. 0 freezes, 1 is normal, other values are
  slow or fast motion.
- The nearest declaring ancestor wins, not a product through nesting
  (Godot's `process_mode` shape): a scene root at 0 freezes the world,
  and a node inside it declaring 1 keeps running (a preview spinning on
  the pause screen). A product cannot express that.
- A frozen track is not frame demand: a paused scene lets the loop go
  idle. A scale write is a JS write, so resuming requests the frame.
- Writes while frozen: undeclared components snap as always; declared
  ones retarget and wait. A destroyed node stays a ghost until its exit
  can play; `spatialTransitionEnd` and frees fire when it settles after
  resume. A node created while frozen sits at its `from` pose.
- Surface: `spatial.setTimeScale(node, scale | null)` and
  `spatial.timeRate(node)` in flux:spatial; `<Scene timeScale>` /
  `scene.setTimeScale()` / `scene.timeRate()` and a `timeScale` prop on
  scene nodes in @solidrt/3d; the 2d layers through their root, groups
  and sprites, since sprites ride the same core transitions. Named after
  Three's `mixer.timeScale` and Unity's `Time.timeScale`.
- Tests: a frozen subtree holds its pose across stepped frames and
  resumes from it; a declaring child keeps running inside a frozen
  parent; a frozen scene reports no demand.
- The docs say what the scale does not cover: the app's own `onFrame`
  logic, timers and `on_advance` modules stay on app time, so app code
  gates itself.

## Design

The first shape of this note had a clock per declaring node, tracks
timed on their domain's clock, and a rebase of a node's tracks when it
was reparented across domains. That was not sound: declaring or
clearing a scale over running tracks is a second domain change the
shape forgot, held writes and staggered exits sat on the same absolute
clock and would have needed rebasing too, and absolute track times were
already what forced the first-stamp `shift` machinery of
[transition-clock-startup-anchor].

**Time lives on the track.** A tween carries how far it has run
(`elapsed_ms`), a spring integrates a step, a held write counts its hold
down (`remaining_ms`). The arena's stamp only says what app time it is;
every track and hold remembers the stamp it was last advanced to
(`since_ms`) and takes `(now - since) * rate(node)` as its step. Nothing
is ever rebased: a reparent or a re-declaration just changes next frame's
step, the first stamp sets `since` on everything that exists (the
startup anchor in one assignment instead of a delta shift), and "a late
hold applies as if started on time" is the overshoot carried into the
new track's lead (`TrackStart`).

**Rate per node.** A node holds `time_scale: Option<f32>` (declared) and
`rate: f32` (effective: its own, else the nearest declaring ancestor's,
else 1). `inherit_rate` recomputes it eagerly on a declaration, a parent
change and an orphaning free, carrying it down to the next declaring
node. Eager, not in the flush walk: the transition advance runs before
the flush in the tick, so a lazy resolve would leak one frame of motion
after a pause, which the stepped tests catch.

**Players take a clock node** at `createPlayer`: a player has one time
over many targets, and its targets are per channel, so "the first
target's rate" would have depended on channel order. The mixer passes
the model's node (Godot's AnimationPlayer-is-a-node). Time, fades and
root motion all take the scaled step; a dead clock drops the player like
a dead target. Players advance before the frame's JS, so a scale set in
frame N reaches them in frame N+1, as any JS write does.

**Demand.** `advance_transitions` and `advance_players` report active
only for tracks, holds, staggered exits and players on a node whose rate
is above 0, so a paused scene idles and `settle` resolves; the plugin's
`setTimeScale` requests a frame when the rate changed, the way
`writeTransform` must.

**Validation.** Finite and at least 0; negative is refused (springs
integrate, and reverse clip playback is a player's `speed`). The dev
clock composes unchanged: app time is already scaled by it, node rates
multiply on top, a step moves app time one period and frozen nodes stay.

**The render tree's element transitions** keep no rate of their own (no
app-facing case: the UI is what must keep moving under a pause, and the
dev clock covers slow motion for debugging), but they move to the same
track model, so the two transition modules stay the line-by-line mirror
they were and the element `shift` code goes too.

**Not built, on purpose:** a mixer per-clip speed change after `play()`
(Three's `action.timeScale`; the core's `setPlayer({ speed })` already
takes it) - a tiny.md line. Element `timeScale`: additive on the shared
track model if a consumer ever appears.

## Findings

Cut to [transition-track-time-model](../notes/transition-track-time-model.md).

Related: [3d-shadow-follows-native-motion].

[3d-shadow-follows-native-motion]: ../backlog/3d-shadow-follows-native-motion.md
[transition-clock-startup-anchor]: ../done/transition-clock-startup-anchor.md
[spatial-node-exit-transitions]: ../done/spatial-node-exit-transitions.md
