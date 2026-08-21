---
title: Isolates for packed and compiled flux scripts
description: fluxrt and packFlux have no isolate resolver and nothing loads flux isolate bytecode, so packed or compiled flux scripts cannot use isolate().
created: 2026-08-21
completed: 2026-08-21
---

# Isolates for packed and compiled flux scripts

## Symptom

A packed flux script (`srt pack --flux`) that calls `isolate()` has every
call reject: `fluxrt` builds its engine without an isolate resolver. And
`--flux --compile` output has no isolate form at all - the standalone `flux`
resolver reads `isolates/<id>.js` source only, so `srt bundle --flux
--compile` ships isolate modules as source beside the bytecode main (and
prints a note saying so).

## Done

- The `flux` binary's resolver tries `isolates/<id>.bin` before
  `isolates/<id>.js` (the lattice resolver's shape); verified by running the
  isolate example with a bytecode-only isolates/ dir.
- `srt bundle --flux --compile` compiles isolates to `isolates/<id>.bin`;
  isolates follow the main bundle's form (source beside .flux.js, bytecode
  beside .flux.bin).
- The fluxrt trailer moved from the single-payload format to the same section
  trailer the solidrt runner uses (packSections; kind-2 file sections only:
  "bundle.bin" + "isolates/<id>.bin"), and fluxrt resolves isolates from the
  payload. Verified: the packed isolate example passes end to end.

The resolver error message differs by host on purpose (file path from
standalone flux, "no such isolate module in this app" from fluxrt/lattice);
all start with `isolate '<id>':`, which is what the example asserts.

Follow-up (same day): trailer parsing and packed isolate resolution unified.
`forge::trailer` owns the section-table parser (the producing half of the
packed-container contract whose consuming half is `fs::AssetsBase::Packed`);
both runners parse through it, each keeping only its own kind dispatch.
fluxrt adopted the solidrt consumption model: it mounts
`AssetsBase::Packed` and resolves isolates by ranged reads through
`flux::resolve_isolate_from_assets`, the same function lattice now uses
(previously lattice's private `resolve_isolate`). Only standalone `flux`
keeps a private resolver (source-first dev semantics, path in the error).
Consequence: in a packed flux script, `assets/` and `isolates/` paths read
through the (index-only) mount and are read-only, no longer plain cwd paths.
