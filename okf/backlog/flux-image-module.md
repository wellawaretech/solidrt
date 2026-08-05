---
type: backlog-item
title: Move the image codec to a forge core behind a flux:image module
description: decodeImage/encodeImage live inline in a lattice-registered global, so headless flux has no image codec; the logic belongs in a forge core marshalled by a thin flux:image module, with core re-exporting like flux:gpu.
status: done
timestamp: 2026-08-05T00:00:00Z
---

# Move the image codec to a forge core behind a flux:image module

Source: the 2026-07-17 image review flagged decodeImage as mis-homed as a
lattice global (recorded in okf/plans/fetch-cache.md, "independently
fixable"); encodeImage (added 2026-08-05) landed next to it and deepened
the debt slightly.

Two homing layers, both currently wrong:

1. The codec logic is inline in the lattice plugin
   (lattice/src/plugins/image.rs). It is pure CPU - encoded bytes to RGBA8
   and back, no GPU, no JS - so by the layering rule (plugins marshal,
   cores own the logic) it belongs in a small forge core
   (forge/src/image.rs) with native-type signatures, next to
   cache/fetch/svg/wasm.
2. The JS exposure is an ambient `image` global registered by lattice, so
   bare flux/fluxrt binaries have no image codec even though nothing about
   it needs alloy or a window. Right shape: a flux:image module in
   flux/src/plugins/modules/, listed in BASE_CAPABILITIES, with
   packages/flux-types/modules/image.d.ts + docs/flux.md parity, and
   packages/core re-exporting from "flux:image" the way gpu.ts re-exports
   captureSnapshot/readTexture from "flux:gpu". The ambient global typed in
   packages/core/src/types.d.ts disappears.

The move: forge gets the image crate dep (decode set
png/jpeg/webp/gif/bmp/ico plus png/jpeg encode) and a ~40-line core with
tests in forge/src/tests/; flux gets the thin module plus an integration
test; lattice deletes plugins/image.rs and shrinks its image features to
["png"] (its only remaining use is the snapshot-reply PNG encode in
go/connection.rs, which could also call the forge helper); core's image.ts
swaps the global for the import. Downstream callers are untouched - the
core export names stay decodeImage/encodeImage.

Cost: bare flux binaries grow by the codec set, roughly what the codecs
add to lattice today. That is the price of headless decode, which is the
point. No back-compat concerns; the global has no external users.

Related: avif-decode.md (the format-coverage gap stays wherever the codec
lives).

Done 2026-08-05, same day: forge/src/image.rs (decode/encode_png/encode_jpeg,
unit tests in forge/src/tests/image.rs), flux/src/plugins/modules/image.rs
("image" in BASE_CAPABILITIES, integration tests in flux/tests/image.rs, incl.
the explicit-undefined options case), lattice plugin + image dep deleted
(go/connection.rs snapshot_reply now calls forge::image::encode_png),
flux-types modules/image.d.ts + docs/flux.md section, core image.ts
re-exports from "flux:image" and the ambient `image` global is gone.
