---
title: Component gestures
description: Press extracted from Pressable into a components-package createPress util and grown into a recognizer family with an innermost-wins arena and first-class cancel and fail.
created: 2026-07-23
completed: 2026-07-23
---

# Component gestures

Grew out of okf/backlog/components-press-util.md ("extract press semantics
into a components-package util; end the Pressable exception"). Scope was
widened in a 2026-07-23 design session: press is the first member of a
gesture recognizer family, and the load-bearing design piece is the
arbitration (cancellation) contract between recognizers, not the press
machine itself. Staging and decisions below are settled; not started.

Two goals in one:

1. End the components-independence exception: 7 components import the
   `Pressable` sibling (Button, Checkbox, Radio, Switch, Select,
   ContextMenu, NavShell; 10 mount sites). They migrate to a shared
   `createPress` util.
2. Lay the recognizer foundation so long-press, pan, pinch, rotate slot
   in later without reworking press or its call sites.

## Background: what other frameworks do

Survey done 2026-07-23 (React Native core + react-native-gesture-handler,
Flutter, SwiftUI/UIKit, web). Common denominators:

- A small canonical vocabulary: tap/press, long-press, double-tap,
  pan/drag, fling, pinch, rotate. Pan is the keystone (scrolling,
  sliders, swipe-to-dismiss all reduce to it).
- A recognizer lifecycle where FAILED and CANCELLED are first-class
  outcomes distinct from ENDED (RNGH: UNDETERMINED -> BEGAN -> ACTIVE ->
  END | FAILED | CANCELLED; UIKit and Flutter equivalent). Press
  feedback appears at began and must be retractable.
- Slop everywhere: movement threshold before a pan activates, movement
  bound before a tap fails, press retention region (RN
  `pressRetentionOffset`), hit slop enlarging targets.
- Long-press = duration timer + movement bound.
- Velocity tracked and delivered at release (fling, momentum handoff).
- Arbitration across the tree, not just bubbling. The defining case is
  parent scroll vs child press: the child shows pressed state, the
  parent's pan activates on slop and CANCELS the child. Every framework
  has a mechanism (RN responder negotiation, RNGH relations, Flutter's
  per-pointer gesture arena, UIKit require(toFail:)). None solve it
  with bubbling + stopPropagation, because the parent must win after
  the child already started reacting.
- The web is the outlier (raw pointer events + capture + touch-action,
  recognizers left to userland libs) - i.e. where we are today, and
  every serious web app ends up pulling in a library.

## Decisions

- **Placement: components package now, core promotion expected later.**
  Rationale (the core-vs-frameworks rule): core is the low-level layer;
  frameworks (components is one) build on it; functionality potentially
  used by ALL frameworks belongs in core. Recognizer machinery (state
  machines, slop, velocity, the cancellation contract) is
  framework-agnostic and thus a core candidate; the visual layer
  (Pressable, style render-prop, d-rect feedback) is components.
  Consequence: press.ts/gesture code must import NOTHING from the
  package's shared modules (no theme, no StyleProps) so promotion to
  core is a file move, not a refactor.
- **Arena in scope now** (user call, 2026-07-23). Innermost-wins
  arbitration; this also resolves the contract question of
  okf/plans/nested-pressables.md in favor of the Flutter-style arena:
  innermost pressable wins the press exclusively, ancestors never fire.
  Raw pointer events keep bubbling regardless; the arena governs
  recognizers only.
- **Extraction is byte-identical first**; the one behavior change
  (capture + press-retention) lands as its own stage after migration,
  verified across all sites at once.
- **No RNGH-style composition algebra** (Race/Simultaneous/Exclusive,
  cross-handler relations). That algebra is a workaround for RN's
  async JS/native split (relations must be declared statically so the
  native side arbitrates without JS round-trips) and for coexisting
  with opaque native views. We have synchronous single-thread dispatch
  over a tree we fully own. Flutter is the existence proof: no
  user-facing operators; the arena's defaults + recognizer config
  cover it, and the genuinely-simultaneous case (pan+pinch+rotate on a
  photo) is a single merged transform recognizer (Flutter's Scale).
  Sequenced (long-press-then-drag, list reorder) becomes an option on
  pan when needed - which is what RNGH's own Pan does too
  (`activateAfterLongPress`). Nothing is designed out: any operator is
  just arena policy, addable if a real screen demands it (design-gaps
  principle: file the gap then, do not pre-build).
- **Scope ceiling per the solidrt lens**: standard vocabulary
  (`onPress`, `onLongPress`, pan/pinch), simplified semantics. Pinch
  and rotate must be EASY to add later (user suspects they will be
  needed; see also okf/backlog/app-wide-zoom.md) - hence the contract
  requirements below - but are not built until a screen wants them.

Contract requirements so later recognizers fit without rework:

- Pointer-SET based, not scalar: pinch is a two-pointer continuous
  gesture (centroid + distance/angle deltas); press uses set-size-1.
  Capture must be able to hold multiple pointers.
- An update phase in the lifecycle from day one (press ignores it;
  pan/pinch stream through it, with velocity at release).
- External cancel as a first-class transition (arena, scroll takeover,
  future root-level recognizers).

## Stages

1. **Extract, byte-identical.** New `packages/components/src/press.ts`
   beside theme/policy/spacing/typography/types: `PressState` moves
   here (pressable.tsx re-exports for compat) plus `createPress`
   owning the exact current semantics (primary-button down + up over
   the same node fires `onPress`; pointer-leave cancels the press and
   clears hover; no capture). Options take `onPress` and the
   pass-through pointer handlers, read at event time so reactive props
   chain. Pressable rebuilt on it, behavior identical. Launcher
   snapshot-check.
2. **Migrate the 7 siblings** off `<Pressable>` onto `createPress` +
   `<view>` + their own d-rect. Each takes over the few lines Pressable
   hid (repaintBoundary, disabled -> pointerEvents none, background
   rect) - deliberate explicitness. Pressable stays exported as the
   app-facing convenience. Still identical behavior; all structural
   moves land before any behavior change. Snapshot-check after.
3. **Arena + lifecycle contract.** The recognizer model lands:
   lifecycle possible/began/active -> ended | cancelled | failed,
   pointer-set based, external cancel first-class. Innermost-wins
   arbitration (nested-pressables resolution; decide ancestor
   press-state styling here too, see that plan's open question).
   Press becomes the first arena recognizer, and this is the one
   behavior-change stage: capture-on-down + press-retention replaces
   leave-cancels (core `setPointerCapture` exists, auto-releases on
   up). Verify mouse and touch paths, launcher trash-icon case.
4. **Later recognizers, on demand** (each its own follow-up, not
   scheduled): long-press when ContextMenu wants it, pan with
   scrollable lists (okf backlog), a merged transform recognizer
   (pan+pinch+rotate) when a screen needs it. Pinch depends on the
   platform prerequisites below.

## Platform prerequisites (recorded, not staged)

Multi-touch plumbing appears already ported from the previous engine
(checked 2026-07-23): alloy translates Finger events to per-finger
pointer ids with touch<->mouse synthesis disabled
(alloy/src/event.rs), the flux gui input plugin tracks hovered paths
per (pointer_type, pointer_id) and clears them on finger-up
(flux/src/plugins/gui/input.rs), core JS capture/routing is keyed by
pointerId (packages/core/src/window.ts). Remaining before pan/pinch:

- Real-device verification (never exercised end to end).
- `is_primary` is missing vs the old engine (~/solidrt-dev/solidrt
  marked the first finger).
- `PointerEvent` carries no timestamp; velocity and long-press timing
  want event time (handler-time now() is skewed by dequeue
  coalescing).
- What desktop trackpad pinch arrives as per platform (often
  ctrl+wheel, which would unify with the desktop zoom convention;
  `WheelEvent` already carries `ctrlKey`).

All are core/alloy-level additions that pass the "used by all
frameworks" test.

## Related

- okf/plans/nested-pressables.md - contract question resolved here
  (innermost-wins arena, stage 3); its survey/verification items stand.
- okf/backlog/app-wide-zoom.md - browser-style whole-app zoom as a
  runtime affordance; a future root-level recognizer that would sit on
  this arena, and the expected motivation for promoting the recognizer
  core out of components.
- okf/backlog/components-press-util.md - the originating item, now a
  pointer here.

## Status

Approved 2026-07-23 (design session: survey, arena-now and
identical-first calls, no-algebra rationale).

Stages 1+2 done 2026-07-23: press.ts extracted (createPress, byte-identical
semantics), Pressable rebuilt on it, all 8 sibling components migrated
(the 7 originals plus SegmentedControl, which had joined since the count
was taken; Button still imports the PressState type for its bg helper).
Verified: launcher renders identical (same node count, 0 orphans) and the
scaffold gallery template exercises every migrated component clean; user
click-through confirmed hover/press behavior unchanged. No component
imports Pressable anymore; it stays exported as the app-facing
convenience. Stage 3 followed (done below).

Stage 3.0 (added by user call 2026-07-23): the getBoundingBox transform
limitation is fixed first, since retention depends on correct bounds.
compute_bounding_box now corner-walks the node's box up through every
ancestor's full View paint matrix (the memoized matrix hit testing
inverts; z = 0 homogeneous divide, translation-only fast path kept) and
returns the AABB of the transformed quad; the node's own transform now
counts (getBoundingClientRect semantics). Unit tests in
alloy/src/tests/tree.rs. Fallout discovered: JS getBoundingBox was
documented window-relative but is positioning-context-relative; docs
corrected and a window-relative `getBoundingBoxViewport` added (flux gui
export + flux-types + core wrapper) - that is the frame pointer
clientX/Y live in, which retention needs. Select/Tooltip/ContextMenu
placement still assumes no positioning context above (latent, open).

Stage 3 implemented and user-verified 2026-07-23: per-pointer claim map
in press.ts (leaf-to-root dispatch makes first-claim = innermost-wins;
plain Map, visible within one synchronous bubble), capture-on-down +
bounds-based press retention (getBoundingBoxViewport), enter/leave
demoted to hover-only, cancel() exposed as the external-cancel hook,
claim released on unmount via onSettled cleanup; no-ref fallback = old
leave-cancel semantics. All 8 components + Pressable attach press.ref.
Probe entry examples/_press_probe.tsx (throwaway, kept for stage 4)
verified nested innermost-wins (ancestor never presses or fires) and
retention (drag-out no fire, drag-back fires). rquickjs trap hit once:
a module export needs a matching decl.declare or the engine fails at
start with "Tried to export a value which was not previously declared".

Found in verification: ScrollView armed its mouse drag on the bubbled
down but a press-claimed pointer routes moves/up exclusively to the
capture winner, so the drag stayed armed forever (ghost scrolling after
releasing a radio). Interim fix: press.ts exports isPressClaimed();
ScrollView skips arming for claimed pointers. Consequence, accepted for
now: a drag starting ON a pressable does not scroll (wheel does). The
stage-4 pan recognizer removes this: pan claims-steals from press on
movement slop (cancelling press feedback), restoring scroll-from-
anywhere - this is now the concrete forcing function for pan.

Stage 4 implemented and user-verified 2026-07-23 (sideways drag from a
gallery button: press feedback retracts at slop, columns scroll; arena
logs confirmed steal + press cancel on every attempt), with a core
routing change enabled by the no-backward-compat call: exclusive pointer
capture (setPointerCapture/releasePointerCapture) is DELETED from core.
window.ts instead freezes the hit path at pointerDown per pointerId;
moves and the up dispatch along that frozen path (same leaf-to-root
bubble, stopPropagation intact) until the up clears it, regardless of
pointer position. Hover moves (no active down) and enter/leave are
unchanged. Consequences: every recognizer on the down path observes the
whole gesture natively, so the arena needs no move-relay/contender
machinery; the ghost-scroll bug class (winner swallows the up, sibling
stays armed) is structurally dead; drags survive leaving the node and
the window; multi-pointer freezing is per-id for free.

On top of it, in components: arena.ts extracts the claims map and adds
claim strength - press claims are provisional (stealable), steal()
cancels the owner and RESOLVES the pointer so it cannot be stolen again
(the outer of two nested scrollers cannot take an inner drag; a second
pan loses and disarms). pan.ts createPan: arm on primary down, activate
on 8px slop, axis-aware ("vertical" pans only activate on vertical
travel, so nested cross-axis scrollers each take their own drags), slop
distance swallowed, deltas stream via onPanMove(dx,dy), cancel() hook,
onSettled claim cleanup; no velocity yet (PointerEvent still has no
timestamp). ScrollView's hand-rolled drag replaced by createPan
(scrollBy(-dx,-dy)); isPressClaimed deleted. Slider drops capture +
stopPropagation and instead steal()s on down (a track down is
unambiguously a slider drag, ancestor scroller can never take it);
frozen-path routing keeps its off-track drag working. press.ts loses
its capture calls and the no-ref leave-cancel fallback (the up always
arrives on the frozen path, claims cannot strand).

Core promotion + merged transform recognizer, 2026-08-09 (trigger: @solidrt/3d
needed pinch-to-zoom - the "used by all frameworks" case this plan predicted).
arena.ts and pan.ts moved from components to core as file moves; core's index
exports the arena as ONE `arena` object (arena.claim/steal/release - bare
claim/steal/release are too generic for core's public surface) plus createPan.
The claims map stays module-global, now guaranteed app-wide single: two arenas
cannot arbitrate against each other, which is why duplication in 3d was never
an option. press.ts stays in components - it has since grown a focus-nav
(registerNavAction) coupling, drifting from the original no-package-local-
imports rule, and nothing outside components needs it; its comment now says
so. Components fallout was three import lines (press.ts, slider.tsx,
scroll-view.tsx).

createTransform (core/src/transform.ts) is the merged pan+pinch+rotate
recognizer this plan anticipated (Flutter's Scale model): tracks the whole
pointer set, arms silently on downs (no claim), activates when focal travel
OR span change crosses the shared 8px slop, then steals every tracked pointer
all-or-nothing (any refusal = the gesture belongs elsewhere, full disarm).
Streams per-event deltas { dx, dy, scale, rotation, x, y }; one finger
degrades to a plain pan (scale 1, rotation 0), set changes (join/lift) rebase
the reference so they never emit a jump, a joining finger is stolen outright.
Rotation pair = first two pointers in down order, delta wrapped to (-pi, pi].

First consumer: @solidrt/3d's createOrbitCamera rebuilt on it - one finger
rotates, pinch zooms (distance / scale^zoomSpeed), wheel unchanged; this also
fixed the latent double-handling (orbit's raw handlers ignored the arena, so
a drag on a viewport inside a scroller both orbited and scrolled). Its
handlers now take core PointerEvent; no in-repo consumers existed.

Platform prerequisites update: multi-touch IS verified in practice - the
trails example paints with multiple fingers (user-confirmed 2026-08-09). Still
open: is_primary, PointerEvent timestamps (velocity), the trackpad-pinch
survey (likely ctrl+wheel, no recognizer needed), long-press on demand.

Verification notes (2026-07-23): the gallery now lives in-repo as
examples/gallery.tsx (+ examples/icon.png), copied from the scaffold
template so it always runs the live workspace packages - a scaffolded
project's copied node_modules go stale, and an orphaned client (its dev
server gone) silently keeps running the last-pushed bundle; check the
served entry before trusting behavior reports. During a pointer-glued
drag the button under the cursor keeps its HOVER tint (pressed is
cancelled, hovered is not; the content tracks the pointer so it never
leaves the button). Open follow-up: suppress hover display while a pan
owns the pointer (mid-drag hover is meaningless; RN/Flutter show none).
Also still open: hover recompute under a stationary pointer while
content scrolls (the hit-test-per-frame gap).
