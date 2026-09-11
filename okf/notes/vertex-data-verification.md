---
title: Verifying engine and vertex-data changes - where the checks stop
description: Four facts about the verification surface met while landing the vertex data model (2026-09-11) - the release client srt run launches, what srt check covers, where the engine matches a pipeline against its program, and half-float support in bun and flux.
created: 2026-09-11
---

# Verifying engine and vertex-data changes - where the checks stop

- The dev server launches the RELEASE client binary. A rig run under
  `target/release/flux -` exercises only the JS side, and a window
  example under `cargo run --example` exercises a debug build, so an
  engine change verified by both can still fail under `srt run` until
  `make client` rebuilds the release binary. The symptom is the old
  vocabulary coming back from reflection (a `programAttributes` name the
  JS table does not know). lattice is its own cargo workspace: a root
  `cargo check -p lattice` does not compile it; `make client` does.
- `srt check <package>` typechecks `examples/`, not `checks/`: a rig is
  runtime-only, and a type change that breaks a rig shows up as a failed
  run, not a failed check.
- The engine matches a pipeline's layouts against the program's
  reflection once, raster-side at pipeline creation
  (`RenderPipeline::new`, by name and component count); the UI side only
  reports `program_attributes` to JS, where the 3d package matches by
  name in `missingAttributes`. A rule about that match therefore has two
  homes.
- `DataView.getFloat16/setFloat16` and `Float16Array` exist in bun and
  in flux's QuickJS, and TypeScript 7 under `lib: ["ESNext"]` knows
  them; half floats need no conversion helper.
