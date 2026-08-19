---
title: "Native transitions: Rust-side animation, JS writes only targets"
description: A transition prop declares per-property motion (tween with CSS curves, or a perceptual spring); the signal path then carries one write per target change and Rust interpolates every frame, taking the measured ~10 us/element/frame JS cost off the frame path entirely. Springs are the retargeting-safe primitive; tweens restart from the current value. Stage 1 covers d-* geometry, opacity and transform components.
created: 2026-08-19
---

# Native transitions: Rust-side animation, JS writes only targets

## Status (2026-08-19): all stages landed

Implemented and verified on the release client: transition config +
tween/spring math in alloy (rendertree/transitions.rs; tree methods in
tree.rs; unit tests in src/tests/transitions.rs), `transition` decode and
the write intercept in flux (properties/transition.rs, tree.rs), animation
clock stamp + advance in lattice (runtime.rs, plugins/draw.rs), JSX types
in core types.d.ts / jsx-runtime.d.ts. Durations are ALL in ms (the
sketch below originally showed a spring in seconds; ms won).

Spec syntax (revised 2026-08-19, flatter than the sketch below): one
level, kind inferred. `{ duration }` and `{ duration, bounce }` are a
spring (bare duration = critically damped - the spring is the default
kind, chosen because it is the retargeting-safe primitive);
`{ duration, curve }` is a tween - a tween is always a named curve, there
is no default curve. `curve` and `bounce` together is a decode error.
The nested `{ spring: {...} }` wrapper shown below was dropped.

Verified with probes/signal-bench.tsx MODE "transition" (1000 d-rects,
spring on x/y, targets once a second): jsMs 11 -> 0.03-0.06 ms at 60 fps,
setPropsPerFrame ~2000 -> ~1, total frame p50 1.7 ms (paint only);
overshoot/retarget continuity visible in /tree samples; with fast-settling
springs the demand gate skips ~55 frames/s between target bursts, so
settled animations produce no frames. Note: a paused clock (scale 0) with
a mid-flight track keeps producing cheap present-only frames (advance
writes nothing, Commit::Reused); a stepped frame advances one period.

## Status: stage 2 landed (2026-08-19)

All four stage-2 items, verified on the release client:

- Color transitions: `color` is animatable (solid colors; a gradient never
  animates and cancels a running color track). Interpolation runs in oklab
  with alpha as its own linear lane; a color spring is four oscillators
  sharing one spec. Tracks are lane vectors ([f32; 4]) internally; scalars
  use one lane. Conversions live in alloy transitions.rs (Ottosson
  constants), round-trip tested. Colors still arrive as packed u32 from
  JS parseColor; whether CSS parsing moves to Rust (dropping colord) is a
  separate open question.
- onTransitionEnd: per settled track, payload `{ property }` (JSX name),
  delivered to the declaring element only, no bubbling. Natural settles
  only - cancels and destroyed nodes never fire; a retarget fires once, at
  the final settle. Path: advance collects settled pairs on the tree,
  lattice draw drains and emits "transitionEnd" engine events per pair
  (flux gui tree.rs emit_transition_ends), window.ts routes to the node's
  handler.
- Damage audit: the animated set already rides the right classes - view
  transform/opacity setters return Compose (caches survive), d-* geometry
  and paint props return Paint, nothing animatable returns Layout except
  w/h on detached primitives (which have no layout anyway). No changes
  needed.
- Coalesced invalidation: advance applies all track damage through
  `apply_damage_batch` - one revision bump per frame and the
  invalidate_paint ancestor walks share a visited set, so N animated
  siblings clear common ancestors once per frame instead of once per
  write. Bench: 1000 animated rects hold 60+ fps, jsMs ~0.04, paint
  ~1.3 ms.

## Status: stage 3 landed (2026-08-19)

All three conveniences, verified on the release client (paused-clock frame
stepping against probes/transition-demo.tsx):

- Shorthand string: `"<duration>ms [curve] [<delay>ms]"` wherever a spec
  object is accepted, plus a bare string as the element-level catch-all
  (`transition="300ms ease-out"` = `{ all: ... }`). Same inference rule as
  the object form: no curve = bounce-0 spring, a named curve = tween. The
  second time value is the delay (CSS order); times are ms only. Bounce,
  bezier control values and `from` need the object form.
- `delay` (ms, both kinds): each write is held for the delay, then applies
  exactly as if written then - a spring retargets from its live state (a
  track mid-flight toward an older target keeps going, and may settle and
  fire its end event, during the hold), a tween starts from the
  then-current value. A newer write during the hold replaces it and
  restarts the delay; a snap write drops it. Holds live on the tree
  (`PendingWrite`), drain at the top of each advance, and count as
  animation-active so the demand gate keeps frames coming during a hold
  (same accepted cost as the paused-clock case above). Delay is
  animation-clock time, so pause/scale/step govern holds too. Staggered
  enter animations are `from` + an index-proportional delay per item.
- Mount-time `from` (per-property entries only; under `all` it is a decode
  error): at the node's FIRST attach - guarded by `Element::entered`, so a
  move or reorder re-runs nothing - the property snaps to `from` and
  animates to the value it mounted with, honoring the entry's delay (the
  element sits at `from` during the hold). A property whose mounted value
  is unreadable (no explicit value, a gradient) skips its enter animation.
  `from` is a number, or a CSS color string / packed number for `color`.

## Status: exit animations landed (2026-08-19)

`exit` on the entry completes the lifecycle pair with `from`: a removed
node stays in the tree, animates each exit property from its current value
to the exit target (entry's spec and `delay` honored - staggered exit
falls out), and is freed when its exit tracks settle. Verified live with
paused-clock frame stepping (toast springs out, node freed at settle, node
count back to baseline, no orphans).

Semantics, all pinned by tests (alloy tests/transitions.rs, flux
tests/properties.rs):

- The renderer's removal path drives it unchanged: `detach_node` finding
  exit entries with somewhere to move marks the node `exiting` and keeps it
  linked; the deferred-destroy sweep finding it exiting defers the free
  (`doomed`) to the settle. A re-insert (Solid detaches before
  re-inserting, so every move passes through detach) abandons the exit:
  moves and reorders never play removal animations.
- An exit whose values already sit on their targets detaches and frees
  instantly - an exit that animates nothing must not defer the removal.
  destroy without detach (forced teardown) is instant too.
- Exiting nodes are hit-test invisible (hit.rs guard): the component is
  disposed, so they must not swallow input on the way out.
- No onTransitionEnd for exits - the handler died with the component.
  Natural-settle events for live nodes are unaffected.
- Only the node the renderer removes animates; its whole subtree stays
  painted with it (a dialog leaves with its contents). Descendants of a
  destroyed subtree never exit on their own.
- Accepted v1 limitation: an exiting ATTACHED element keeps its layout
  slot until the exit finishes, so siblings close the gap at settle, not
  gradually (animating layout is a separate, much bigger item). Detached /
  absolute / overlay elements - what exits are mostly for - have no slot.
- Re-showing mid-exit mounts a fresh node (Solid semantics): briefly the
  old one leaves while the new one enters, AnimatePresence-style.

Nothing remains; see "Deliberately out of scope" for what was consciously
excluded.

The compositor-side-animation item (finding b in
notes/app-structure-performance.md), shaped after the update path was
measured (notes/signal-to-setproperty-path.md): an animated element costs
about 10 us of JS per frame, ~75% of it Solid's generic reactive machinery,
and no per-hop optimization changes the scaling term. Moving interpolation
below the FFI does: the JS cost becomes per target change instead of per
frame, and during a running animation the signal path does not run at all.

## Shape

One path, not two. The state path stays exactly what it is - signal,
effect, `setProp`, `setProperty` - and the `transition` prop adds a time
dimension to it on the Rust side:

```tsx
<d-rect
  x={target()}
  transition={{ x: { duration: 200, curve: "ease-out" }, y: { duration: 400, bounce: 0.2 } }}
/>
```

- The transition config is a property OF THE ELEMENT, keyed by property
  name, with `all` as a catch-all for every animatable property. It
  applies to every subsequent write of that property.
- A write to a property with a transition starts (or retargets) a track
  from the current value; a write without one snaps, as today.
- The initial value never animates (matches CSS; a `from`/enter story is
  a later stage).
- Rejected alternative: animated-value wrappers (`x={spring(target())}`)
  - per-write configs, allocation per change, and an invitation to move
  time back into JS.

## Tweens and springs

- **Tween**: `{ duration, curve, delay? }`, curve one of `linear`, `ease`,
  `ease-in`, `ease-out`, `ease-in-out`, or `[a, b, c, d]` cubic bezier
  (the named ones are beziers too; one evaluator).
- **Spring**: `{ spring: { duration, bounce } }` - perceptual parameters
  only (SwiftUI's model): `duration` (ms) the perceptual settling time,
  `bounce` in (-1, 1] with 0 critically damped. Physics parameters
  (stiffness/damping/mass) are deliberately NOT exposed; the perceptual
  pair maps onto them internally (mass 1, stiffness = (2*pi/duration)^2,
  damping = 4*pi*(1-bounce)/duration for bounce >= 0, and for bounce < 0
  the overdamped form damping = 4*pi/((1+bounce)*duration)). If a real
  need for raw physics tuning ever appears it can be added as an
  alternative object shape without breaking this one.

Retargeting (a new target while a track runs):

- Spring: state is position + velocity, the new target changes the
  equilibrium, motion stays continuous. This is why the spring is the
  primitive for anything interactive.
- Tween: restart from the current value with the full duration (CSS
  semantics). Additive/delta composition (Core Animation style) is a
  possible later stage, noted under out of scope.

## Rust side

- A track list on the rendertree: `(node, property, from, to, spec,
  start; springs carry velocity)`. The frame driver advances all tracks
  before layout each frame, writes through the existing typed setters so
  damage classification stays correct, drops settled tracks, and keeps a
  frame requested while any track runs - settled animations stop
  producing frames, so demand-driven rendering stays honest.
- The transition config is decoded once in the plugin layer
  (`transition` is a normal property write); the track holds resolved
  state, so the per-frame advance touches no strings, no `apply_jsx`,
  no marshalling. Rendertree stays engine-free: tracks are native types,
  the plugin only decodes the prop.
- Animatable set, stage 1: numeric scalars - d-* geometry (`x`/`y`/`w`/
  `h`, line endpoints), `opacity`, transform components (`x`/`y`/
  `rotate`/`scale`/`scaleX`/`scaleY`/`rotateX`/`rotateY`), `strokeWidth`,
  `radius` (single-number form). Transforms animate as components, never
  as a matrix. Colors need an interpolation space (oklab) and are
  stage 2.

## Stages

1. **Core.** Track list + frame-driver advance; tween evaluator (bezier)
   and perceptual spring; `transition` prop decode (per-property + `all`);
   retarget semantics as above; JS types in core `types.d.ts`. Verify
   with `probes/signal-bench.tsx` reworked to write targets once a
   second instead of every frame: `jsMs` ~0, `setPropsPerFrame` ~0
   between target changes, fps steady, motion visually continuous under
   retargeting (snapshot probes).
2. **Breadth + per-frame cost (done, see Status).** Color interpolation (oklab);
   `onTransitionEnd`; audit damage of the animated set - animated
   transform/opacity must ride `Damage::Compose` (recording kept,
   applied at composite), not Paint; coalesce per-write ancestor
   invalidation into a per-frame dirty drain (the `invalidate_paint`
   walk is O(depth) per write and becomes the dominant per-frame cost
   once JS is out of the path).
3. **Conveniences (done, see Status).** Shorthand string form
   (`transition="200ms ease-out"`), `delay` (springs and tweens),
   mount-time `from` (enter animations).

## Deliberately out of scope

- Keyframe animations (time-driven, CSS `animation`): a different item.
- Additive tween retargeting (Core Animation): only if CSS-style restart
  visibly stutters in practice.
- Physics spring parameters as API (see above; mapping documented so the
  door stays open).
- JS-driven per-frame motion (procedural animation, physics from input):
  stays on the existing path; its cost ceiling is the signal-path work,
  not this item.
- Native scroll physics (finding c): separate item, but should reuse the
  spring/track machinery.