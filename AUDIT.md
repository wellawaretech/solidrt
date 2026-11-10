# Audit

What to check when auditing solidrt. One section per rule: the rule, where it
is stated, and how to check it. This file holds the checks only. Findings are
work: file them in `okf/` (`tiny.md` or `backlog/`), not here.

## No outside project is named

Rule: CLAUDE.md "Never name an outside project". Nothing in the repository
names or points at a project that uses SolidRT (an app, a game, a client's
product or the maintainer's own): not its name, path, ids, assets or
screenshots, and no code or tuning values copied from it. A problem found
there is described by its shape and reproduced here with neutral names.

Scope: the whole tree, `okf/` and the tests included, and the commit
messages since the last audit.

How to check:

- Ask the maintainer for the names and paths of the projects built on
  SolidRT. Keep that list out of the repository: it is what the rule
  forbids.
- `git grep -il <name>` each of them over the tree, and
  `git log -i --grep=<name>` over the history.
- Read the provenance phrases added to `okf/` since the last audit ("seen
  in", "found in", "reported by"): one that points outside is a finding
  even where it gives no name.

Not a violation:

- Third-party software, dependencies and prior art named for comparison.
- The examples, demos and apps that live in this repository.

## Flux plugins marshal only

Rule: CLAUDE.md "Plugins should be thin FFI layers", `flux/CLAUDE.md` "Plugin
layering". A plugin decodes JS arguments into native types, calls the owning
crate (forge, ore, alloy), and encodes the result back. Domain logic belongs
in the owning crate, exposed as a method the plugin forwards to.

Scope: `flux/src/standards_plugins/`, `forge_plugins/`, `ore_plugins/`,
`alloy_plugins/`.

How to check:

- Read the diff of the plugin layers since the last audit, then the largest
  plugin files (`wc -l flux/src/*_plugins/*.rs | sort -n`). Logic grows in
  the large files.
- Flag code that computes rather than converts: geometry or pixel math,
  thread loops and pumps, state machines, queues, caches, retry or timing
  policy, tuning constants that are not about the JS surface.
- For each flag, ask: could the owning crate expose this as a method that
  takes and returns native types? If yes, it is a finding.

Not a finding:

- Argument validation and option decoding (defaults, exclusive options,
  error messages naming the API).
- Engine resource bookkeeping: tracking created ids so engine teardown
  releases them, `Persistent` registries, `PendingOps` holds.
- Logic of the JavaScript engine itself, which no other crate can own
  (forge names no engine types): timer and idle-callback scheduling, child
  runtimes for `flux:isolate`.
- One-line adapters from one crate's trait to another crate's handle, where
  flux is the only place both crates meet.

## 2d and 3d speak one language

Rule: `packages/2d/CLAUDE.md`, `packages/3d/CLAUDE.md`. A concept both
packages have carries one name, one shape and one meaning on both sides,
and behavior that is not about the dimension lives in core. Each package
lands features on its own, so the two drift apart between passes
(`okf/done/2d-3d-vocabulary-drift.md`,
`okf/done/2d-3d-record-layers-one-model.md`).

Scope: `packages/2d`, `packages/3d`, and the core modules they share
(`@solidrt/core` residency, camera-control, input, the shot blend).

How to check:

1. One concept, one name. Pair the root exports of
   `packages/2d/src/index.ts` and `packages/3d/src/index.ts` by role:
   handles, events, options, verbs, constants, components. Flag a pair
   that differs by more than the dimension marker (`Camera2d` against
   `OrbitCamera` is fine, `RayHit` against `Hit` was not). Reading state
   follows one convention: clock state is a method, a description is a
   field.
2. Same verb, same shape. A verb both packages export takes and returns
   the same shape: argument names, single-or-list, option bags, the
   handle type it accepts.
3. Same component, same props. Compare the Props types of the
   components with a counterpart: `Scene`/`SpriteLayer`/`TileLayer` and
   `View3d`/`View2d` (size, fill, `output`, `oversample`, `maxPixels`),
   `Group`, `Shot`/`Shots`, `Region`/`RegionGrid`, and the node props
   (`visible`, `renderOrder`, `timeScale`, transitions, pointer
   handlers). A prop with the same name means the same thing and takes
   the same unit.
4. Complete exports. Every type a public function or component takes or
   returns is exported from the package root, `components/index.ts`
   included. Core functions the docs tell an app to call are re-exported
   by both packages or by neither.
5. Dimension-free code goes to core. For each file name present in both
   `src/` trees, count the lines the two copies share. A high share asks
   the CLAUDE.md question: is this behavior about the dimension? If not,
   it is a finding to move it down. Materials and GLSL are about the
   dimension and stay. "Core" is two layers, and a piece goes to the
   lowest one that owns it:
   - The Rust spatial core (`alloy/src/spatial/`): per-node or
     per-frame work over arena state that needs no JS. Reusable by any
     extension built on the arena, and by any engine.
   - `@solidrt/core` TS: generic orchestration that must call app JS
     (async loads, generator builds) or Solid lifecycle. It takes
     package things as plain values or type parameters (a `NodeId`, a
     handle type `R`), never as an interface the package implements.
   - The package: anything that touches its own representation (its JS
     mirrors of arena nodes, its GPU resources, its byte measure).
   The smell: a move into core that needs a callback bag describing the
   package's object model belongs lower or is not generic. Hooks that
   say WHEN (events) are fine; operations that say HOW are the smell.
   Ask also whether a third-party 2d or 3d extension could use the
   piece unchanged; if only our two packages can, it is shared package
   code, not core.
6. Tests mirror. A behavior both packages have is tested on both sides
   under the same file name (`records`, `native-motion`, `fill-knobs`,
   `time-scale`, ...). List `tests/` on both sides and flag a test whose
   concept exists on the other side without one.
7. Cross-references resolve. Every "@solidrt/3d's X" in the 2d docs
   (`AGENTS.md`, `agents/*.md`, `README.md`) names a live 3d export or
   doc section, and the reverse:
   `grep -n "@solidrt/3d" packages/2d/AGENTS.md packages/2d/agents/*.md`.

Not a finding (settled; the reason is the model, not history):

- Creation grammar and removal. A slot-bound thing is added into its
  owner and can only be destroyed: 2d `addSprite`/`addGroup`/`addText`,
  3d `addInstance`. A free-standing node is created, then added, and can
  be detached: 3d `createMesh` + `add`, `remove`. A sprite cannot exist
  outside its layer, so 2d has no detach; a sprite that should come back
  is hidden.
- One bag for a sprite, one setter per aspect for a mesh. Both pose
  writes are bags (`setSprite`, `setTransform`). A mesh's other aspects
  cost differently (a material swaps the pipeline; layers and shadows
  have their own passes), so each has its own setter; a sprite's aspects
  are all cheap record writes.
- Shared names. Same concept, same name, even though the two roots then
  export the same identifiers (`Sprite`, `Group`, `worldPosition`,
  `records`, `createShots`, ...); an app importing both aliases one. The
  finding is the reverse: a shared name with two meanings.
- `pick` returns `Sprite[]` in 2d and `Hit[]` in 3d. A 2d pick is a point
  in a rect, so a distance or a normal would be invented; a 3d pick is a
  camera ray. 2d `raycast` returns `Hit[]` like 3d's.
- The mixer's `clips` is a field. It describes the model; clock state
  (`playing()`, `frame()`) is a method on both sides.
- A 3d `blend` implies `transparent` and the back-to-front sort; a 2d
  layer has no depth and draws in record order.
- 2d camera drivers take one target or a list (`createCamera2d`,
  `createShots`), 3d's one. A 2d scene is several layers, each with its
  own view, sharing one camera; a 3d scene is one draw.
