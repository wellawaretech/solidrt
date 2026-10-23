# @solidrt/2d - motion

Retargeted (native) motion, frame-rate motion and frame animation. Read
../AGENTS.md first: it carries the model this file builds on.

## Retargeted motion

Retargeted motion is NATIVE: `setSpriteTransition(sprite, { position:
{ duration: 700, bounce: 0.3 }, ... })` (or the `transition` prop) makes
setSprite writes TARGETS the core animates toward - position/scale
(w/h) on the shared spring/tween math, rotation along the quaternion
geodesic (always the short arc; a spring keeps its velocity through
retargets). JS costs one write per target CHANGE, zero per frame; the
running tracks drive frame demand themselves, and settled sprites cost
nothing (bench: 400 retargets ~4ms, once a second - vs ~5ms per FRAME
moving the same population imperatively). Retargeting every frame is
also a legitimate pattern, not an abuse: rewriting a spring's TARGET
each frame to chase a moving point (a follow-camera trailing a moving
sprite) rides the spring's smoothing for free - a spring keeps its
velocity through retargets, so the chase stays fluid. A mount pose snaps
unless a component's `from` (2d units: `position: { from: [x, y] }`,
`rotation: { from: angle }`, `scale: { from: [w, h] }`, a group's
`from: s`) animates the sprite in from there - once, and only when the
declaration lands in the tick that added the sprite (the `<Sprite>` prop
or a setSpriteTransition right after addSprite; a later one animates
writes only). A component's `exit` (same units) is where it animates to
when destroySprite/destroyGroup lets go of it: the sprite stays drawn
while it leaves and is a GHOST meanwhile - no pick, raycast, overlap,
sweep or pointer event sees it - then frees when the last exit settles
(a bullet never hits a corpse; an entering sprite is live from its
first frame, whatever `from` it passes through). Either endpoint takes
the object form `{ value, duration?, curve?, bounce?, delay? }` to own
its direction's motion (an ease-out enter, an ease-in exit), and `delay`
on an entry holds its writes that long on the animation clock (a late
frame catches up, so a hitch never shifts a held start). `stagger` (ms)
goes on a GROUP's declaration (`setGroupTransition` / `<Group
transition>`), never on the sprites: it spaces the enters and exits of
everything under the group that begin in one frame by `index * stagger`
(add order in, tree order out whatever the destroy order; nearest
declaring group wins), and
cascades nothing unless the sprites declare `from`/`exit` - a hand of
cards fanning in, a menu popping in, is one `stagger` on the group. For
the sprites straight under the layer (no group) the layer's root carries
it: `createSpriteLayer(texture, { stagger })` / `layer.setStagger(ms)`.
Each natural settle calls the handle's `onTransitionEnd` (plain field,
or the `<Sprite>`/`<Group>` prop) with `{ component }` - target-only,
never on a cancel, snap or exit; the raw "spatialTransitionEnd" engine
event (sol:events, node = sprite.node) stays for flux:spatial consumers.
See examples/springs.tsx (tap a sprite: it leaves through its `exit`).

PAUSE and time scale are declarations on the same nodes:
`layer.setTimeScale(scale)` / `<SpriteLayer timeScale>` sets the rate
every sprite and group transition in the layer runs at - with their
delays, enters, exits and stagger slots - 0 a pause that leaves the
app's UI moving, 1 app time, 0.5 slow motion; `setGroupTimeScale` /
`<Group timeScale>` and `setSpriteTimeScale` / `<Sprite timeScale>`
declare it for a subtree or one sprite. The NEAREST declaring node wins,
never a product (Godot's process_mode shape): a frozen layer with a
`<Group timeScale={1}>` inside keeps that group running. A frozen layer
asks for no frames (the write that resumes it brings the frame), writes
while frozen retarget and wait, a destroyed sprite with an `exit` stays
a ghost until its time moves again (the layer's `dispose` still cuts it
short), and a sprite added frozen sits at its `from`. The scale covers
the CORE's motion: frame animations follow it only when given a `clock`
(see Frame animation), and your own `onFrame` logic, timers and audio
stay on app time - multiply your step by `layer.timeRate()` /
`timeRate(sprite)`, the effective rate read. The dev clock
(`/clock?scale=`) scales app time underneath all of this.

## Frame-rate motion

Frame-rate motion only JS can compute (physics, flocking) bypasses the
declarative layer: `ref` the sprite, call `setSprite` from `onFrame` (a
~7us core transform write per moved sprite - fine to a few thousand;
past that use the records layer). Signals carry structure and slow
state - a `<Sprite x={sig()}>` re-running 60 times a second works but
re-runs an effect per sprite per frame for nothing.

## Frame animation

Frame animation (animation.ts): `createAnimation(frames, fps, { loop })`
is a clip with a shared clock on the app's frame time (it freezes with
the dev clock and plays at its own speed under `sol render`) stepping
every attached sprite
(`anim.add(sprite)` / `remove`; `play`/`pause`; `frame()`/`playing()`
read the clock, methods like the 3d mixer's `playing()`; `loop: false`
holds the last frame and fires `onFinish`, the mixer's name for it in
@solidrt/3d). One no-demand frame callback per playing clip, one
setSprite per sprite per STEP - an 8fps cycle is 8 publishes and 8
presents a second regardless of display rate, and a paused clip costs
nothing. A sprite belongs to at
most one animation (add detaches the previous); removed sprites prune
lazily on the next step. Works on both layer kinds; with `<Sprite>`,
attach via `ref` and leave the `frame` prop off - the clip owns that
field (the prop effect passes absent props as undefined, which setSprite
keeps, so other props stay reactive). A clip is a JS clock, so a layer's
`setTimeScale` does not reach it by itself: `createAnimation(frames, fps,
{ clock: layer })` (a group or a sprite works too) runs the clip at that
handle's `timeRate`, read every frame, so the one pause that freezes the
world's transitions holds its walk cycles as well, and a slow-motion
group slows the flipbooks in it.
