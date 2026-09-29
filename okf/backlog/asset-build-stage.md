---
title: Assets are baked by hand, one tool run per file
description: A compressed texture or a baked model exists only after the developer runs `srt tool` on its source and puts the output under assets/, so sources and outputs drift and nothing rebakes when a setting changes; a build stage that bakes on the copy into dist/<flow>/assets/ (with a manifest that makes it incremental) removes the step, and must be designed before compressed textures outside a model are built because it decides how an app names a baked file.
created: 2026-09-30
---

# Assets are baked by hand, one tool run per file

What it looks like when you hit it: the project holds `Sponza.gltf` and
its images, the app loads `assets/sponza.srtm`, and the second exists
only because somebody ran `srt tool 3d/model ... --ktx2` and waited a
minute. Change `solidrt.textures` in package.json and nothing happens
until every model is baked again, by hand, one at a time. A model whose
source changed and was not rebaked ships stale.

Unity and Godot import by themselves: the app names the source, the
editor keeps the imported form beside it and redoes it when the source
or its settings change. Three leaves it to a command-line tool
(glTF-Transform, the KTX tools), which is where we are.

From [gpu-compressed-textures](../plans/gpu-compressed-textures.md),
decided 2026-09-30 to be a plan of its own. It was an ideas.md line
waiting "on an actual transform to exist"; compressed textures are that
transform.

## What done looks like

`srt pack`, `srt render` and `srt bundle` bake what the project's assets
need on the copy into `dist/<flow>/assets/`, and redo only what changed.
The developer keeps sources in the project and runs no tool. The bake
tools stay, as the primitive under the stage and the way to not use it.

## What it involves

- **How an app names a baked file.** The decision everything else hangs
  on, and the reason this is designed BEFORE compressed textures outside
  a model are built: if the stage turns `assets/tiles/ground.png` into a
  KTX2, the path the app wrote must still work. So a loader takes a path
  and tells the format by content (`isKtx2`), never by extension. A
  model is the harder case: the app names `assets/sponza.srtm`, the
  source is `Sponza.gltf`.
- **The manifest.** A plain copy can be dumb, a re-encode cannot: Sponza
  bakes in 65 s. Per staged directory, keyed by the source's hash and
  the settings that applied to it (so a change to `solidrt.textures`
  rebakes what it touches and nothing else).
  [build-output-dirs](../done/build-output-dirs.md) has the staging
  this sits on and names the manifest as its consequence.
- **What dev serves.** The dev server resolves assets live from the
  project root. Either it bakes on the fly in its asset route, or it
  folds into the same staging with a watcher. What a developer tests
  should be what ships.
- **Tools declare what they take.** A stage can only run a bake it knows
  the inputs of. `srt tool` discovery lists tools and nothing more
  (and, today, only under `@solidrt/*`: see the plan's "Tools of
  third-party extensions"). A tool needs a declared contract: which
  files it takes, what it writes.
- **Where it runs.** Every bake that encodes textures runs under flux
  (the encoder is the runtime's), and so does everything that matches
  the patterns of `solidrt.textures.files`: bun's matcher and ours
  differ.

## Order

Designed before "compressed textures outside a model" is built; built
after tool discovery, because it has nothing to run until the texture
bake and the tool contract exist.
