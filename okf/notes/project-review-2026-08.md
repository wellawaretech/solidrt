---
title: Project review 2026-08
description: Whole-project session review (2026-08-28) vs Flutter/RN/Electron; core renderer bet right, product layer (tests, text editing, a11y, security, docs) is the gap; 8 ranked priorities.
created: 2026-08-28
---

# SolidRT: high-level review

**Snapshot:** ~68k Rust + ~25k TS, one author, first commit 2026-03-07 (5.5 months), 1,129 commits, 61 tags since May, 5 platform packages published from CI. 58 MB runtime binary, 682 crates in the lock file.

## Verdict in one paragraph

The core architectural bet is right and unusually well executed: fine-grained signals driving a retained Rust render tree, with damage tracking, a dedicated raster thread, native transitions, owned text layout, and a GPU that the app can actually program. That is a real differentiator against all three incumbents, and the 3D/2D/shader story is something none of them have in the UI tree. The engine layering (forge / alloy / flux / lattice) is principled and enforced, not aspirational. What is *meh* is almost entirely the "product" layer: the surface an app developer touches has no tests, thin human docs, no accessibility, no text selection/clipboard, no lazy lists, no navigation model, no permissions model, a non-JIT engine, and a bus factor of one. Today it is an excellent platform for agents, games, kiosks, TV, visualization and embedded UI; it is not yet a platform an outside team could ship a business app on.

## What looks good

**1. The renderer core is the strongest part.**
- Damage is a *returned value*, not a side effect: every setter returns `Damage` and [tree.rs](alloy/src/rendertree/tree.rs) `edit()` forces the closure to return one, so forgetting invalidation is a compile error. The five-level ladder (Present < Compose < Scroll < Paint < Layout) is precise and `apply_damage_batch` de-duplicates ancestor walks.
- The record order (matrix, clip, scroll, fit, children) is documented once in [composite.rs](alloy/src/rendertree/composite.rs) and hit-test, cull, bounding-box and paint all mirror it. This is the invariant most home-grown renderers get wrong.
- Raster thread contract: one ordered channel, explicit fire-and-forget vs RPC, Kahn-ordered shader target rendering with cycle detection (unit tested), load shedding, present fences, GL state save/restore around Impeller. Zero `.unwrap()` outside tests.
- Own [layout cache](alloy/src/rendertree/layout/cache.rs) fixes a real taffy weakness (single slot clobbered by parent-width variants).
- Owned text layout is pure arithmetic over pre-measured runs; floats, exclusions, justify, balance/pretty, ellipsis are all there and tested with synthetic metrics.

**2. The engine layering is real.** `grep rquickjs forge/src` is empty. Every forge core returns a neutral `Value`; the seams (hyper `Service`, `FnMut(TcpStream)` spawn callback, `&mut dyn FnMut` host dispatcher for wasm/ffi) are principled. Event-loop details are careful: HTML-spec unhandled-rejection semantics, microtask drain after every exec so event N+1 sees N's writes, documented rquickjs hazards patched with upstream bug reports in `okf/upstream/`.

**3. The app-author API has good bones.** Explicit layout / transform / paint split, native springs and transitions on every property (including 3D scene nodes and sprites), a pointer model better than the DOM's (localX/parentX, frozen down-path, one gesture arena), element-granular error containment, a real theme + policy layer that reshapes components by form factor, TV-grade spatial focus navigation. The `viewBox` design-space trick and the shader-per-texture / view-shader / window-shader tiers are genuinely cheap to use.

**4. Tooling and process are ahead of the project's age.** Multi-device push (LAN, iroh tunnel), reload-on-save keyed on the bundle input set, a control API + 19 MCP tools with frame stepping and time scaling, headless render, a release pipeline that builds 5 targets and has a pack-and-verify guard born from three broken releases. 602 Rust tests, consistently in `src/tests/`. The `okf/` state-by-directory system is disciplined and the agent-facing `AGENTS.md` files are the best docs in the repo.

## What is meh

**Renderer**
- **No partial repaint, no layer tree.** Every frame re-rasterizes the full window display list; boundaries save DL rebuild and snapshot raster, but a 1 px change is still a full-screen raster + resolve. Flutter's layer tree and Chrome's tiling/damage rects both avoid this. On a phone or a MediaTek TV this is the ceiling on battery and fill rate.
- **O(n) per-frame scans**: [tree.rs:1037](alloy/src/rendertree/tree.rs#L1037) walks every node whenever any GPU texture changes (a camera or video frame per tick over a large tree); `set_unrounded_layout` calls `invalidate_paint` per moved node with no batching, so a resize is O(n * depth).
- **Two text engines coexist** (legacy `ParaCache` + `OwnedCache` on every Text); caret stops shape every grapheme prefix (O(n^2) per word, kerning lost); shaping goes through Impeller single-line paragraphs so there is no cross-word kerning/ligatures and no own font fallback.
- **No effects exposed**: Impeller has blur, backdrop, shadows, color filters, path clips, masks; the rendertree exposes none. The escape hatch is custom GLSL, which is a power-user answer to "I want a drop shadow".
- `unsafe impl Send/Sync` by assertion on `RenderTree`, `PlatformContext`, `Context`; `node()` panics on an unknown id (a JS ordering bug aborts the process); 201 `Result<_, String>` sites and no error enum; [raster/mod.rs](alloy/src/raster/mod.rs) 1.9k lines with a 500-line match, `App::run` ~500 lines with ~25 mutable locals, seven things named `*Context`.
- Tests are shape-only: none run taffy or GL; `composite.rs` has one test.

**Runtime**
- **No security model at all.** Any app bundle gets `flux:ffi` (dlopen + libffi + read/write arbitrary process memory, temp `.so` at a predictable path, [ffi.rs:259](forge/src/ffi.rs#L259)), `flux:subprocess`, `process.env`/`kill`, unconfined `flux:fs`, raw sockets. `BASE_CAPABILITIES` is a feature list, not a grant. Bytecode is loaded with `unsafe Module::load` and packs are unsigned, so a tampered pack is memory-unsafe. The dev client executes whatever `reload.code` it receives with no auth. Fine for a lab; not fine for "one installed client runs many apps", which is one of your headline claims.
- **Idle spin**: [engine.rs:438](flux/src/engine.rs#L438) sleeps 1 ms and re-selects whenever `PendingOps` is held, i.e. a 1 kHz wakeup for the life of any open server/socket. Headless `fluxrt` servers pay this permanently.
- Main engine has no memory limit or interrupt (only isolates do); `ExecHandle` is an unbounded channel; `livekit-wakeword` is patched from a personal fork.
- REDESIGN stage 3 is half done: `ui_thread` in [lattice/src/lib.rs](lattice/src/lib.rs) is a ~600-line closure with ~40 `#[cfg(feature = "go")]` interleavings; flux still depends directly on hyper/reqwest/iroh.
- Android: `extractAssets` copies the whole APK asset tree on every `onCreate`; no lifecycle hooks beyond visibility.

**App surface**
- **Zero JS tests** across ~29k TS lines: renderer glue, bundler, components, press state machine, scroll clamping, text buffer are all verified by hand. `okf/backlog/js-test-infrastructure.md` admits it.
- **Business-app blockers**, in the order a dev hits them: no text selection or clipboard anywhere (`grep clipboard` over the Rust tree is empty); no navigation stack / screen model; no list virtualization, no fling, no scrollbar, `Select` option list not scrollable; no form validation, `Option.value: unknown`; no `Intl` (QuickJS has no ICU); no accessibility of any kind (zero hits in the whole tree except one unrelated README line); no notifications, share, keychain, deep links, background execution.
- Reactivity traps leak to users: two-arg `createEffect`, `REACTIVE_WRITE_IN_OWNED_SCOPE`, microtask-deferred reads, and the element-prop double-read that is a *native memory leak* with no analogue in web Solid. The scaffold `AGENTS.md` is honest about this, but a human developer reads that list as "three ways to shoot myself on day one".
- Components are looser than core: `children?: any` throughout, doc drift (Button docs say content-sized, code says `width: "100%"`; `policy.ts` says no Tab, `focus-nav.ts` implements Tab). Portals/modals cannot mount on first render.
- Layout gaps vs CSS reflexes: grid without `fr`/`minmax`/areas, `position: absolute` resolves against nearest `relative`, `lineHeight` is a multiplier, a `<view>` cannot paint its own background.

**Project**
- **Bus factor 1** and `CONTRIBUTING.md` says not accepting contributions. That is a bigger adoption blocker than any feature gap.
- CI tests run on Ubuntu only; macOS/Windows/Android are build-only; no clippy, no fmt check, no rendered-output assertions, website build not gated. `NPM_TOKEN` is live in a job that runs `bun install`.
- Human docs: 512 lines across 5 pages, one tutorial (a counter), `architecture/index.md` ends with "per-crate pages are not written yet". No testing, deployment, signing, navigation, perf, a11y or i18n guides. CHANGELOG starts at 0.0.45; 44 releases have no notes.
- Commits land as 30-100-file sweeps with one-line subjects; the memory index tracks many things as "DONE uncommitted".
- Repo root is a lab bench: `app copy.tsx`, `SCRATCH`, `TODAY.md`, `earcraft.db`, `video copy/`, 350 KB of probe bundles, untracked test directories, `okf/feedback/` (private customer feedback) one `git add -A` away from being published.

## Versus the alternatives

| | SolidRT | Flutter | React Native | Electron |
|---|---|---|---|---|
| Rendering | Retained Rust tree, Impeller, full-window raster per frame, author-declared boundaries | Widget/Element/RenderObject + layer tree, engine-managed raster cache, Impeller | Native platform views via Fabric shadow tree | Chromium: layerization, tiling, damage rects |
| Reactivity cost | O(delta): a signal edits one node | Widget rebuilds, then diff | VDOM diff + shadow-tree commit | VDOM (usually) + DOM/style/layout |
| Layout | taffy flex/grid/block; no lazy viewports | Constraints-down; slivers, lazy lists | Yoga flex; virtualized lists | Full CSS; virtualization via libs |
| Text | Own LTR breaker, floats, balance; no bidi, no selection, no fallback | SkParagraph: bidi, fallback, selection, editing model | Platform text | Full browser text |
| GPU access | Draw lists, instancing, targets, vertex+fragment shaders, shaders over UI content, 3D in tree | Fragment shaders only; 3D is an island | Effectively none | WebGL/WebGPU in a canvas rect |
| JS engine | QuickJS-NG interpreter, no JIT, no ICU | Dart AOT/JIT | Hermes (tuned interpreter, static Hermes) | V8 |
| Debugging | Custom ws + MCP: tree, snapshots, input, frame stepping, time scale; no breakpoints/profiler/heap | DevTools, Observatory | Hermes debugger, Flipper, RN DevTools | Chrome DevTools |
| Capabilities | sqlite, fs, serve, p2p, subprocess, ffi, wasm, audio, video, camera, speech in the runtime, identical on every target | Platform channels + pub.dev plugins | TurboModules + community | Node + Chromium APIs, desktop only |
| Permissions/sandbox | None | OS permissions | OS permissions | contextIsolation, session permission handlers |
| Accessibility | None | Semantics tree, first class | Native a11y | Full browser a11y |
| Platforms | Linux, macOS, Windows, Android (incl. TV, Pi); no iOS | All six + web | iOS/Android, desktop via forks | Desktop only |
| Footprint | ~58 MB single binary, no Chromium | ~10-20 MB + Dart VM | App + JS bundle | 150+ MB |
| Dev loop | Push to every connected device at once, no rebuild, agent-native | Hot reload per device | Fast refresh per device | Reload |
| Maturity | 0.0.x, 5.5 months, 1 maintainer, 0 JS tests | Stable, Google | Stable, Meta | Stable, OpenJS |

Where you are genuinely ahead: rendering architecture cost model, GPU/3D/shader integration in the UI tree, TV/Pi/kiosk as first-class targets, batteries in the runtime, multi-device dev push, and agent-driven inspection. `DIFFERENTIATORS.md` is accurate and appropriately restrained about what is not a differentiator.

Where you are behind, and not by a little: text editing (selection, clipboard, IME composition, bidi), accessibility, lazy lists and scroll physics in the engine, permission/security model, JS throughput, debugger/profiler, human documentation, and organizational durability.

## What I would prioritize

1. **JS test harness in CI** (already shaped in backlog). Every other item below is unsafe to ship at your commit cadence without it.
2. **Text selection + clipboard + IME composition.** This is the single feature gap that disqualifies the most app categories, and it touches alloy, flux and components at once, so it only gets more expensive later.
3. **Decide the security posture explicitly.** Either the launcher runs trusted apps only (say so, drop the "one client runs many apps" framing) or ffi/subprocess/process/fs need a per-app manifest grant. Signing the pack container is on the same path.
4. **Fix the 1 ms idle spin and the O(n) texture scan.** Both are small and both bite on the exact devices you target (TV, Pi, phone).
5. **Lazy/virtualized list + engine-side fling.** Flutter's slivers are the reason it feels native on long lists; a column that "IS the list" does not survive a real dataset.
6. **Expose Impeller's filters** (blur, shadow, backdrop, path clip) as props. Cheap to do, closes the "where is box-shadow" reflex without GLSL.
7. **Docs and hygiene**: 10x the human prose, a real tutorial past the counter, clean the root, commit or delete the untracked tests and examples, protect `okf/feedback/`.
8. **Bus factor**: at minimum a contribution path and a written "how the three trees relate" architecture page. The `okf/` record makes onboarding a second maintainer feasible; the current contributing policy makes it impossible.

Nothing here is a rewrite. The foundation is the right one; the work is filling the product layer that the foundation was built to make cheap.