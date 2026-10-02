---
title: Stage A review - what could be better
description: An assessment from building instanceOrder on the 3d meshes plus the core direction feed (gaussian-splats stage A, 2026-09-26) - the wider-scope calls first (triple record residency, upload-vs-index ordering, the JS-side camera funnel, scene.ts's entry-lifecycle web, the verification story), then the narrow punch list of pre-existing breakage, accepted costs and warts.
created: 2026-09-26
---

# Stage A review - what could be better

An assessment from building gaussian-splats stage A, written for triage:
first the wider-scope items - architecture and process calls I would
weigh differently, or that this work exposed - then the narrow punch
list. The what-was-built record is
[../done/gaussian-splats.md](../done/gaussian-splats.md)'s findings.

## In a wider scope

- **A retained ordered population holds its records THREE times.** The
  JS stream mirror (mesh.ts, every population has one), the core's
  retained order mirror (context/order.rs, the `retain: true` copy), and
  the GPU buffer. At stage B scale that is ~84 MB resident for 28 MB of
  splat records - before the fetched source bytes are dropped. Each copy
  was added locally for a good reason (partial rewrites; core-side
  re-sort; the draw), but nobody chose the sum. For write-once
  populations the JS mirror is pure dead weight. I would give the record
  layer a hand-off form - records transferred to the engine at creation,
  no JS mirror, partial rewrites refused by contract - and stage B
  should decide this BEFORE loadSplat lands, because "fetch, upload,
  drop" is the whole point of the baked format. The same review question
  generalizes: whenever two layers each keep a mirror for their own
  feature, the residency is multiplicative and invisible.
- **Ordering moves data where the rest of the industry moves indices.**
  The settled gather design re-uploads the whole population per re-sort
  (28 MB per camera turn at 1M records); the web viewers upload a 4-byte
  index per record and leave the data still. gpu-instance-order
  considered and parked that escalation on gpu-float-texture-formats,
  which has since shipped - so the prerequisite is gone and only demand
  gates it. The gather design is still the right default (zero shader
  contract change, and the epsilon gate makes idle free, which the
  viewers lack), and `retain` already made strategy a knob, so an
  `index` strategy is additive. But I would have written the plan's
  stage C with "re-upload bandwidth at 1M" as an explicit go/no-go for
  building it, instead of "last resort" phrasing - the measurement is
  cheap and the answer decides real architecture.
- **The camera funnel is JS-side and keeps widening.** This stage added
  one more per-camera-move consumer in the core (the order feed), fed
  from the view the scene already pushes down - which was only possible
  because the spatial core holds per-target views at all. But the
  pattern is now: every camera move runs JS that writes uViewProj
  params, setView, shadow matrices, and (indirectly) triggers cull, LOD,
  entry sort and record order. The camera-controls work already moved
  the CONTROL core-side; the remaining wider move is a core-owned camera
  per target - JS writes pose and lens, core derives matrices, frustum,
  LOD view, shared params - collapsing the per-move JS channel to one
  write. Each new consumer (this one included) makes that refactor
  cheaper to justify; none of us has done it because each increment is
  small. Worth a design/ document the next time a camera consumer lands.
- **scene.ts is where entry lifecycle goes to hide.** The ordered-entry
  work touched attachScene, _attach, _setLayers reachability, and the
  publish path, and the hardest part of the change was re-deriving the
  attach/detach web (2700 lines, five entry points with distinct
  reachability). Shadows already escaped into makeShadowSystem; entry
  lifecycle (attach/detach/rebuild/view fan-out) deserves the same
  extraction before the next subsystem lands on it. This is the one
  refactor I would have seriously considered doing FIRST if stage B did
  not already queue behind it.
- **Verification leaned on me, not on the repo.** The end-to-end proof
  of this stage is an untracked probe asserted by hand-run curls, and
  the previous end-to-end guard (draw_ordered.rs, also untracked) had
  already rotted unnoticed - the plan even named it as coverage without
  anyone checking it still compiled. Two lessons at plan scope: when a
  plan cites an artifact as its verification vehicle, verifying that
  artifact works belongs to the planning step; and the probes that prove
  core behavior should graduate into something CI builds and runs
  ([../done/test-harness.md](../done/test-harness.md)
  is exactly this gap). I kept the session honest by running everything
  myself, but nothing prevents the next regression from arriving
  silently.
- **Three hand-synced declarations per boundary feature.** `orderFeed`
  exists in the Rust plugin parse, in flux-types' BindDrawOptions, and
  behaviorally in the core - the flux-types parity problem in miniature,
  kept in sync by discipline and one memory rule. Fine at this rate of
  change; if the GUI surface keeps growing, generating the `.d.ts` from
  the plugin declarations is the structural fix and would have caught
  nothing here but will eventually catch something.

## Pre-existing breakage found along the way

- `alloy/examples/draw_ordered.rs` (uncommitted) does not compile: it
  predates the open-vertex-layouts API churn (`PipelineDesc.attributes`,
  `DrawSpec.buffer`, `BufferUpdate.instance_buffer`; 18 errors). It was
  the end-to-end guard for all three gpu-instance-order stages, so the
  engine-level retained-order path currently has no compiled
  self-asserting example - the lib tests and probes/order-3d-probe.tsx
  carry the coverage. Porting it (and adding a feed section while there)
  is the right fix. The general lesson: untracked examples carry real
  verification value and rot silently because nothing builds them.

## Decisions I would revisit when a consumer forces it

- `ORDER_DIRECTION_EPS_COS` (~2 degrees, alloy/src/spatial/mod.rs) is
  set by feel and is one constant for every consumer. Stage C's
  measurement (re-sort plus republish cost at 1M records on the phone)
  should set it; if particles and splats end up wanting different gates,
  it becomes a per-binding option - additive.
- `descending` is explicit on the 3d `instanceOrder` (core semantics
  passed through), yet every position-key consumer imaginable wants
  back-to-front. I kept it explicit rather than defaulting position keys
  to descending; `createSplatMesh` will hardcode it in stage B, so apps
  rarely see the knob. Flip the default only if it keeps getting
  forgotten in practice.
- A mesh drawn ONLY by a view (layered out of the scene target) never
  re-sorts: one buffer holds one order and the scene entry owns it. This
  is structural, documented in packages/3d/AGENTS.md, and fine for
  splats; if a real app shows an ordered population exclusively in a
  view, the ordered entry would have to become "the first entry
  created", which I rejected here for its lifecycle complexity.
- The core `InstanceOrder` API speaks FLOAT offsets while instance
  records have been byte-packed since 3d-instance-records-as-bytes: a
  key behind a half/unorm prefix that is not 4-byte aligned is
  inexpressible (both layers refuse it with a clear error today). If a
  packed record ever needs such a key, the flux boundary should switch
  to byte offsets - breaking, but nothing depends on the float form
  outside our own code.

## Costs accepted by design, and when they would bite

- An ordered mesh publishes the LIVE record set whole on any write:
  one `setInstanceStyle` on a retained ordered instanced mesh sends
  `count * stride` bytes and re-runs the core gather (plus the sibling
  republish on a permutation change). Irrelevant for splats (records are
  write-once) and for per-frame populations (they republish whole
  anyway); it would bite a large, mostly-static population taking
  frequent single-record style pokes. That population should think twice
  about ordering at all.
- A gather-strategy (non-retained) ordered population that republishes
  every frame sorts with the direction fed at the PREVIOUS flush:
  publishRecords runs before spatial.flush in the scene sync. Bounded at
  one frame and invisible in practice; if particles ever care, the fix
  is feeding direction before the record publish in the sync, not a new
  mechanism.
- A non-retained ordered mesh's FIRST publish sorts by the seed
  direction (`ORDER_SEED_DIRECTION`, scene.ts) until the first feed
  lands one flush later; a retained mesh self-heals in the same sync
  (the feed's rematerialize re-gathers). The scene cannot compute the
  true initial direction itself - it lacks the node's WORLD rotation,
  which is exactly why the feed lives core-side.
- A splat cloud drawn into a reflection probe renders in scene-camera
  order (the probe's face views never feed). Marginal by construction;
  recorded so nobody chases it as a sorting bug.

## Tooling and test gaps

- Probes cannot self-assert on GPU buffer contents: `readBuffer` is
  Rust-only by design, so order-3d-probe.tsx is verified externally via
  `/buffer` curls instead of printing OK/FAIL like retain-probe does via
  pixels. Worked fine, but a dev-only buffer readback in `sol:dev` would
  make ordered-population probes self-contained. Low priority: the
  control API flow costs two curls.
- `/buffer` reads cap at 64 KiB: fine for three quads, useless against a
  28 MB splat buffer. Stage B verification should assert on windowed
  reads (offset + length at the head, middle and tail of the gathered
  set), not full readbacks.
- The context-level direction-write path (Writer::write_order_direction
  routing to rematerialize, with its RefCell ordering) has no Rust test;
  the spatial tests stop at the recorder and the live probe covered the
  context path. The ported draw_ordered example is the natural home.
- Adding a `SinkWriter` method meant touching three implementors, and
  testing per-kind write failure grew the test Recorder a
  `dead_directions` list beside `dead`. If the trait keeps growing,
  generalize the recorder's failure injection by write kind instead of
  one list per method.
- Adding a field to `DrawSink` meant editing thirteen struct literals in
  tests. The `sink(draw)` helper exists; the literals predate it. Worth
  sweeping onto helpers next time someone is in that file, not worth a
  dedicated pass.

## Small warts left in on purpose

- `attachScene` is reachable from `_attach` AND from `setLayers`, which
  is easy to miss: my first placement of the ordered-mesh whole-republish
  sat in `_attach` and missed the layers path until re-checked. The
  attach/detach web in scene.ts (attachScene / attachView / _attach /
  _setLayers / _setCast, each with its own reachability) is the file's
  hardest part to hold; a short comment map at attachScene now carries
  part of it, and any future entry-lifecycle work should start by
  re-deriving who calls what.
- order-3d-probe.tsx registers its debug commands inside the component
  (they need the scene context) where the debugging guide recommends
  module init; correct for a probe that mounts once and never hot-swaps
  its scene, but not a pattern to copy into long-lived apps.
- The 3d option and the core option share the name `instanceOrder` with
  different vocabularies (attribute names vs float offsets, no direction
  vs required direction). Deliberate - the package speaks names, the
  engine speaks layouts - and documented, but reading code across the
  boundary you must keep track of which shape you are holding.
