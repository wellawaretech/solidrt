# Performance model (JS is the slow lane)

Read this before writing any per-frame code, any animation, or anything that
writes properties in a loop.

The JS engine is interpreted and every property write crosses an FFI boundary
into the runtime, so per-frame JS work is the expensive path while GPU work is
nearly free. That holds on desktop and on current mobile hardware; "Where GPU
work stops being free" below is where it does not. The design answer is not
"write less JS" but "keep JS off the per-frame path": the platform animates
(transitions), caches (repaint boundaries), shades (GPU) and computes
(isolates, wasm) natively, and JS stays the coordinator that sets targets.
Rules, in order of leverage:

1. Motion between states (position, size, opacity, transform components,
   solid colors, enter/exit) belongs in a native transition, never in
   per-frame JS. Declare `transition` on the element and keep writing
   targets the ordinary way; the runtime interpolates every frame on the
   Rust side, so JS runs only when a target changes and the running
   animation costs no JS and no property writes per frame, however many
   elements move. Flat spec, ms durations, kind inferred:
   `{ duration }` / `{ duration, bounce }` is a spring (the default kind;
   springs carry velocity, so a retarget mid-flight stays continuous -
   use them for anything interactive), `{ duration, curve }` is a tween
   (`linear | ease | ease-in | ease-out | ease-in-out` or a cubic-bezier
   array; tweens restart from the current value on retarget, CSS
   semantics). Keys are property names plus `all` as catch-all; a string
   is shorthand (`transition="300ms ease-out"`); `delay` holds each
   write, `from` animates the first attach in (enter), `exit` animates
   removal out before the node frees, `stagger` on a parent cascades its
   children's enters/exits, and `onTransitionEnd` fires per settled
   property. The initial value never animates without `from`; a write to
   a property without a transition snaps, as always. A JS tween loop or
   animation library pushing interpolated values through signals pays the
   whole write path per element per frame - port it to this.
2. Continuous effects (snow, particles, animated backgrounds) belong in a
   fragment shader: createShaderTexture (from @solidrt/core/gpu) + `<texture
   params={{ uTime }}>` (the shader declares `uniform float uTime;` itself -
   the preamble declares only what the runtime fills). The whole effect then
   costs one setProperty per frame - the uTime write - regardless of visual
   complexity. Shader output
   must be premultiplied alpha (white flakes are `vec4(vec3(a), a)`);
   straight alpha (`vec4(1,1,1,a)`) composites as opaque white. A source that
   starts with `#version 300 es` is compiled exactly as written - no preamble
   is injected, though the built-in vertex stage still supplies `vUV` - so a
   shader ported from elsewhere keeps its own uniform names without dropping
   to compileShader/linkProgram. Params drive any uniform type: a number
   fills a `float`/`int` scalar, a flat number array fills `vec2`/`vec3`/
   `vec4` (2/3/4 numbers) or `mat4` (16, column-major), dispatched by the
   shader's own declaration - a ported shader's `vec2 uCenter` or
   `vec3 iResolution` needs no splitting into scalars. To combine several
   GPU passes, stack `<texture>` elements and set `blendMode` (e.g. a base
   pass plus an additive `blendMode="plus"` pass) rather than writing a
   compositing shader. Within one pipeline draw, createPipelineTexture's
   `blend: "add"` accumulates overlapping geometry additively (soft point
   splats, glow) - pair it with `depthWrite: false` when depth-tested;
   neither option implies the other. A pipeline's own vertex stage writes
   into a y-down clip space: `gl_Position` y = -1 is the top row of the
   target and +1 the bottom, so camera-up geometry must negate y (or fold
   the flip into its projection) or it draws upside down. Sampling is a
   create-time option on every texture: `{ filter: "nearest" }` for
   hard-pixel upscaling (render a small target, display it big - the
   retro/pixel-art path) and `{ wrap: "repeat" }` to tile outside 0..1 in
   shaders; the defaults are linear and clamp, and the choice applies both
   on screen and to shaders sampling the texture.
3. Reduce setProperty calls wherever possible: one path string rebuilt per
   frame beats N elements with N animated positions; a shader beats the path
   string. get_stats' setPropsPerFrame is the counter to watch. Compiled JSX
   attribute expressions diff before writing, so a per-frame expression that
   returns an unchanged value costs no property write - setPropsPerFrame
   counts values that actually changed, not expressions re-run.
4. Never leave onFrame registered while nothing animates: a pending onFrame
   is a standing frame request, so the runtime renders and presents every
   vsync even when the callback body does nothing - an invisible 60fps GPU
   burn that also drags the OS compositor along with it. Tweens and
   springs need no pump at all - that is rule 1, and the runtime requests
   frames only while tracks run. For genuinely procedural per-frame motion,
   use a self-rechaining one-shot requestAnimationFrame that stops
   re-requesting when its work list empties. (Registering onFrame outside a
   component body also warns NO_OWNER_CLEANUP - it assumes a reactive owner.)
5. repaintBoundary works like Flutter's: transforms and opacity on the
   boundary node itself (or any ancestor) are hoisted out of the cache and
   applied at composite time, so animating x/y/scale/rotate/opacity of a
   boundary does NOT re-raster it (verified by A/B measurement - the damage
   system classifies these as Transform and keeps the node's own cache).
   What DOES invalidate the cache is any paint or content change inside the
   subtree - colors, path data, text, a Show toggling - so drive animation
   with transforms and keep the cached content itself static. Off a boundary,
   `opacity` on a view is NOT cheap: it wraps the subtree in a compositing
   layer (save_layer) for as long as it is below 1. To fade a single
   primitive, put the alpha in its `color` (`rgba(...)`) - paint alpha is
   free; reserve view `opacity` for fading a genuine group as a whole.
   Placement rule for animation-heavy screens: a boundary around a node
   that animates its own paint (a moving d-*, a changing color) is useless
   - its interior is damaged every frame, so the cache never survives. The
   win is a boundary around the static bulk NEXT TO the animators: the
   frame then re-records only the moving nodes and replays the fenced
   content as one cached draw, an order-of-magnitude cut when static
   content dominates the node count. get_stats' nodesPainted shows exactly
   what the paint walk still enters. The exception where a boundary on the
   animator itself pays is transform/opacity animation of the boundary
   node - the hoisting described above.
6. "snapshot" boundaries pay first-frame texture allocation + raster:
   creating many at once (dealing a board of 64 sprites) is a visible
   one-frame hiccup - pool or pre-warm if that moment matters.
7. Shading pixels the app already drew is a different mechanism from rule 2's
   generated textures, and both forms are a `shader` prop taking a linked
   program from compileShader/linkProgram (@solidrt/core/gpu), not a
   createShaderTexture source. On `<window>`, `shader={{ program, params }}`
   runs the finished frame through the program as the last step before it
   reaches the screen: the frame binds as `uniform sampler2D uSource`,
   `iResolution` fills by name, and `previous: true` retains the last frame as
   `uPrevious` for motion echo or frame differencing. On a `<view>` the same
   prop shades that subtree in place and REQUIRES repaintBoundary="snapshot"
   (without it the shader is ignored with a warning); the pass sees only the
   subtree's own pixels - grading, warping or dissolving the panel works,
   anything needing what is behind it does not - and is split from content
   invalidation, so a params-only change re-runs the pass against the cached
   snapshot instead of re-rasterizing. A window shader's output is invisible
   to get_snapshot and every other MCP tool; `bunx srt render` is the only
   way to see it (see @solidrt/cli AGENTS.md).
8. `flux:wasm` runs a pure interpreter (wasmi, no JIT), so temper browser
   expectations - but do not write it off for compute. A genuinely numeric
   kernel (typed-array math, tight inner loops, no host calls inside the
   loop) compiled from a systems language can come out a real multiple
   faster than the same loop in interpreted JavaScript, and when profiling
   shows such a kernel is what the app is spending its time on, that
   multiple is worth having: measure the JS loop, port the kernel, measure
   again, keep whichever wins. What wasm does not do is speed up
   render-path work (rules 1-3 are that leverage), and every host call
   costs marshalling, so batch at the boundary - one call over a byte
   buffer, not a call per element. It is also the way to ship one compiled
   module across every target with no native toolchain, and it pairs with
   an isolate when a call runs long enough to block.
9. `flux:ffi` (dlopen of a native library) is a binding tool, not a
   performance tool. It needs a shared library compiled per platform and
   architecture and shipped under each target's packing rules (Android
   loads only what arrives inside the APK as a lib*.so), so reaching for
   it "to make something fast" buys a build-and-packaging problem on every
   platform the app targets. Use it when the app must call a native
   library that already exists and already ships for those targets; for
   speed, everything above comes first.

## Isolates: heavy work off the JS thread

A long synchronous computation (a big parse, a simulation step, a blocking
`flux:ffi`/`flux:wasm` call) freezes rendering and input for its duration.
Move it into an isolate module: a file whose first statement is the
`"use isolate"` directive runs in a second runtime on its own thread, and
main calls its exports as async functions.

```ts
// src/worker.ts
"use isolate"
export function crunch(data: Uint8Array): number { /* ... */ }
```

```ts
// src/index.tsx
import { isolate } from "flux:isolate"
import type * as Worker from "./worker"
let worker = isolate<typeof Worker>("worker")   // id = path from src/, no extension
let n = await worker.crunch(bytes)              // main keeps rendering meanwhile
```

The bundler builds each such module as its own bundle and ships it with the
app (dev pushes and `srt pack` alike). Rules: main may only `import type`
from an isolate module (a value import is a build error); arguments and
results are copies (numbers, strings, byte buffers, arrays, plain objects -
no functions, no class instances); the child has the non-gui `flux:*`
modules only, so it never touches the render tree; module state persists
between calls and each `isolate()` call is its own instance. An
`async function*` export is a stream: `for await (let p of worker.progress())`
pulls one item per step (progress, ticks, a subscription), `break` ends it in
the isolate, and streams never block plain calls. Full contract:
node_modules/@solidrt/flux-types/modules/isolate.d.ts.

## Where GPU work stops being free

"GPU work is nearly free" is a property of the hardware, not of the engine, and
the spread is wide enough to design against rather than discover late. The same
app - two point-cloud pipelines, 233,600 vertices, one params write each per
onFrame, i.e. exactly what rule 2 recommends - measured 16.7 ms/frame (60 fps,
vsync-locked) on both desktop and a mid-range 2020 tablet, and 120 ms/frame
(8.3 fps) on a 2017 Android TV. Roughly 8x for identical work, with the tablet
indistinguishable from desktop. Measure on a target device if it matters; do
not infer it from the desktop number.

- **On a tiled GPU the budget is primitive count, not pixels.** Every point or
  triangle costs the tiler regardless of how few pixels it covers. On that TV,
  frame time against total vertices with a trivial vertex shader: 20k -> 80 ms,
  35k -> 100 ms, 100k -> 380 ms. Meanwhile `gl_PointSize = 3.0` - nine times
  the fill - measured within one vsync of 1.0, and rendering into a
  quarter-size target measured identical to full size. So for a heavy pass the
  lever is fewer primitives; shrinking the target or the splat usually is not,
  and coverage is far cheaper bought with point size than with more points.
- **A device's compositor can set the frame budget outright**, in which case
  none of the above moves. That TV never presents faster than every 80 ms -
  four refresh periods blocked inside `eglSwapBuffers` - even for a near-empty
  scene, so its ceiling is ~12 fps whatever you draw. Recognise it by a
  content-independent floor: if a trivial scene and a heavy one present at
  nearly the same rate, you are compositor-bound and tuning the scene is
  wasted effort.
- **Per-frame writes are gated on the raster thread, not on vsync**, so a pass
  that costs more than a refresh period does not silently pile up. If
  `rasterQueue` climbs across queries while fps drops the raster thread is
  behind; if `fenceTimeoutsPerSec` (in get_stats' window block) is nonzero,
  the GPU is over its pacing budget right now.

Finding your own numbers: `get_stats` gives fps, frameMs, setPropsPerFrame,
the window summary (worst frame, percentiles, GPU rates), rasterQueue and
fenceTimeouts. When those disagree with what the screen is
visibly doing, ground truth on Android is
`adb shell dumpsys SurfaceFlinger --latency <layer>` for real present
timestamps - engine-reported phase timings can each be honest and still not add
up to the frame period, because work outside the frame call is not in them.
