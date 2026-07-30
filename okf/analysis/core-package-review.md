---
type: analysis
title: Core package review
description: Best docs and layering in the repo; gaps are zero tests, docs teaching nonexistent props, a throwing onFrame killing sibling animations, and silently black invalid colors.
timestamp: 2026-07-15T00:00:00Z
---

# Core package review

Full review of `packages/core` (~2.2k lines of TypeScript: 18 src modules, two
ambient .d.ts files, jsx-runtime.d.ts) as of 2026-07-15. Every source file
read; `parseColor`'s failure behavior verified by running colord; every
re-exported Solid symbol (Repeat, Loading, Errored, Reveal, onSettled,
createStore, ...) verified present in the installed 2.0.0-beta.17. Companion
to the forge, alloy, flux, and cli reviews.

## Summary

This is the thinnest layer in the stack and reads like it: a renderer bridge
(ProxyNode tree + deferred destroy with move semantics), event dispatch with
bubbling and pointer capture, reactive env/capabilities state, and a family of
createX device primitives that all follow the same owner-scoped lifecycle
pattern. Doc comments are the best in the repo - they consistently explain the
non-obvious (the ownedWrite rationale, the cleanup-before-await trap, why the
first frame bootstraps off the sticky resize event). Layering discipline holds
throughout: the headless mechanisms (createScroll, createTextBuffer) carry no
UI opinion, and every createX convenience names its imperative flux:*
counterpart instead of hiding it.

The gaps: zero tests (true of every JS package, but core has the purest,
highest-value candidates); docs/core.md - which the README calls the full API
reference - actively teaches props that do not exist (`fill`, `background`,
`stroke`); AGENTS.md has drifted in four spots; a throwing onFrame callback
permanently silences every other animation registered that frame; and an
invalid color string silently paints opaque black.

## Completeness

The surface is coherent and covers the intended "low-level toolkit" scope:
rendering + JSX intrinsics, pointer/wheel/key/text/focus events, window state,
env/capabilities, gamepads, color and gradients, headless scroll/text-input
mechanisms, portal, images, the GPU tier (textures, shaders, pipelines,
buffers), and camera/microphone/sound/speech as subpath modules. Holes and
asymmetries:

- **KeyEvent is `{ key: string }` only** - no modifier flags, no repeat, no
  code. Keyboard shortcuts (ctrl+s, shift+arrow selection at the app level)
  cannot be expressed. Compounding it, key events are delivered only to the
  focused node and dropped when nothing is focused, so app-global shortcuts
  are impossible - even though `onKeyDown` is typed on every element via
  PointerProps, on most elements it can never fire.
- **Type/intrinsic mismatches, both directions.** `LineProps` is exported but
  `<line>` is not a registered intrinsic (AGENTS.md documents this honestly);
  `svg`/`d-svg` ARE registered intrinsics but `SvgProps` is missing from the
  index.ts type exports, so a typed wrapper component cannot import its props.
- **createCaretScroll measures with fontSize only.** CaretScrollInput has no
  fontFamily/fontWeight/fontStyle, so a skin using anything but the default
  font computes caret positions against the wrong glyph widths and the scroll
  drifts from the real caret.
- **The index-vs-subpath split is mostly deliberate** (device modules are
  subpaths; ubiquitous things live in the index) but gpu straddles it:
  `createTexture` is in the index while `createShader`/`createPipeline`/
  `createBuffer` are subpath-only. Fine if intentional; worth one sentence of
  policy somewhere.
- Focus traversal (tab order) is absent - already a known backlog item.
  `Sound.playing()` not tracking natural completion is documented, with
  onended deferred.

### Documentation drift

- **docs/core.md is wrong, not just stale.** It shows `<view background=...>`,
  `<rect fill=...>`, `<oval fill=...>`, `<path fill=...>`, and `<line
  stroke=...>` - none of these props exist (the model is `color` +
  `drawStyle`, which AGENTS.md teaches correctly and even warns that "some
  older doc examples are wrong about this" - this is that doc). It also uses
  `imageWidth`/`imageHeight` on `<texture>` (actual: `w`/`h`), fillRule
  casings `"nonZero"`/`"evenOdd"` (actual: lowercase), omits onFrame's third
  `rate` argument, and covers perhaps a third of the real surface (no
  env/capabilities, gamepads, gradients, pct, scroll, portal, text-input, or
  any subpath module). The package README links it as "the full API
  reference".
- **AGENTS.md** (otherwise the best doc in the package) has four drifts: it
  lists `audio` as a registered JSX intrinsic (there is none; audio is the
  flux:audio module), omits `svg`/`d-svg` from the registered-intrinsics list
  while teaching `<svg>` two sections earlier, names the subpath
  `@solidrt/core/speech` (actual: `/speech-recognition`; `/sound`, `/image`,
  `/text-input` go unmentioned), and puts `cx`/`cy` in the TransformProps
  bucket (actual: `originX`/`originY`).
- **examples/README.md** omits `gpu-pipeline.tsx`, which sits in the folder.

## Code quality

High. The defects found are edge-of-the-happy-path, but two are in the frame
loop where they hurt most:

- **A throwing onFrame callback permanently stops other animations.** runFrame
  swaps the callback map and each callback re-registers itself only after its
  fn returns. A throw therefore (a) skips re-registration of every
  not-yet-run callback in that batch - they never fire again - and (b) skips
  `flush()` and `renderFrame()`, dropping the frame. One misbehaving callback
  silently kills every animation in the app. A per-callback try/catch (log
  and continue, matching what the DOM does for rAF) is a few lines.
- **Same class in pointer dispatch**: `bubble`/`dispatchOrdered` call handlers
  bare, so one throwing handler suppresses delivery to the remaining targets,
  and for pointerDown also skips the outside-tap blur that follows the loop.
- **Cancelling an onFrame from inside another onFrame is not honored** when
  the cleanup is invoked synchronously in the same tick: the victim's entry
  lives in the already-swapped local map, the stored frameId is stale, and
  its extendedFn then re-registers - the "cancelled" callback runs forever
  unless cleanup is called a second time. (Reactive disposal is safe: it runs
  in the post-loop flush, after re-registration has updated frameId.)
- **Pointer captures are not cleared when the captured node is destroyed.**
  destroyNode clears focus but not pointerCaptures (window.ts), so all
  further moves for that pointerId are swallowed - routed to a handler that
  no longer exists instead of hit-tested targets - until the next pointerup.
- **parseColor fails soft to opaque black, silently** - verified:
  `parseColor("notacolor")` === 0x000000FF. A typo'd color renders as black
  with no signal anywhere. This is one instance of the open fail-soft-decode
  question in the property API design; a dev-build console.warn would pay for
  itself the first time.
- **createImage never checks `response.ok`**: a 404 feeds the error page's
  bytes to decodeImage, surfacing as a baffling decode error rather than
  "404 for <url>".
- **createTextBuffer.setSelection clamps only the upper bound** - a negative
  anchor/focus passes through (Math.min against length, no Math.max 0) and
  flows into String.slice on the next edit. move() clamps correctly; this one
  path does not.
- **Sound voices accumulate**: every play() pushes a voice handle and only
  stop() prunes; a long-running app with a frequently triggered overlapping
  sound (game sfx) grows the array without bound.
- **Minor**: `nodes` and `getEventHandler` are exported from renderer.ts with
  zero consumers anywhere in the workspace - either internal-only leftovers
  to unexport or public API missing its doc comment. attachWindow hand-rolls
  13 unsub variables where an array would do. windowFocused/keyboardHeight
  signals skip the `ownedWrite` opt-out their sticky-event siblings carry -
  worth verifying those events are genuinely non-sticky.

Strengths worth naming so they survive refactors: the deferred-destroy sweep
with re-insert cancellation (DOM-move semantics in ~40 commented lines); the
first-frame bootstrap comment chain in attachWindow explaining exactly why
flush() is illegal at that point; createImage's generation guard plus
register-cleanup-before-await; the Solid 1.x deprecation stubs typed `never`
so removed APIs surface as IDE strikethroughs with migration hints instead of
"not exported" errors; and the curated re-export block that deliberately
avoids `export *` from solid-js.

## Tests

**There are none** - no test files, no test script in any package.json in the
workspace. Core is the best place in the JS side to start: unlike cli, much
of it is pure logic with no process to spawn. The flux:*/srt:* builtin
imports are the only obstacle, and `bun test` module mocking (mock.module)
covers them. Candidates in value order:

- `createTextBuffer` (text-input.ts): controlled vs uncontrolled, maxLength
  clamping, selection collapse/extend semantics, the range math - pure, zero
  mocks, and would have caught the negative-offset clamp today.
- The renderer tree ops (renderer.ts) with a mocked flux:rendertree: insert
  with/without anchor, remove-then-reinsert cancelling the pending destroy,
  the subtree sweep skipping moved-out descendants. This is the subtlest
  logic in the package and exactly the kind that regresses silently.
- `parseColor`/gradient stop parsing (color.ts): the packed u32 layout, alpha
  scaling, the black fallback (pin current behavior, whatever is decided).
- `capabilities.windowSizeClass` thresholds and the inputDevices-vs-seen
  fallback logic, with a stubbed srt:events.
- `createScroll`/`createCaretScroll` clamp-and-follow math with stubbed
  getBoundingBox/measureText.
- onFrame re-registration + dispatch (window.ts) with a stubbed event bus -
  locks in whatever throw policy comes out of the fix above.

## Improvement points, ranked

1. Rewrite docs/core.md (or generate it from types.d.ts) - it is the linked
   API reference and it teaches props that do not exist. Cheapest correct
   move: gut it to match AGENTS.md and the types.
2. Harden the frame loop and pointer dispatch against throwing callbacks
   (per-callback try/catch + console.error; keep flush/renderFrame running).
3. Add tests, starting with text-input, color, and the renderer tree ops via
   mock.module (an afternoon; the package is already factored for it).
4. Fix the four AGENTS.md drifts (audio intrinsic, /speech path, cx/cy,
   svg/d-svg in the intrinsics list) - it is the doc agents actually trust.
5. Extend KeyEvent with modifiers and decide the global-shortcut story
   (window-level key delivery when nothing is focused would cover most).
6. Small defects batch: response.ok in createImage, setSelection lower-bound
   clamp, clear pointerCaptures in destroyNode, prune finished voices (or
   land onended), dev-warn on unparseable colors.
7. Export SvgProps; add font options to CaretScrollInput; unexport (or
   document) `nodes`/`getEventHandler`.
8. Hygiene: add gpu-pipeline.tsx to examples/README.md; drop or explain the
   `_imgtest.tsx` scratch file before it ships in the npm `files: examples/`
   glob.