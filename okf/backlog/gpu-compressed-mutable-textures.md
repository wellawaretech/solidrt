---
title: A block-compressed texture cannot be rewritten in place, so a streamed world's baked textures are created anew on every enter
description: createMutableTexture and uploadTexture refuse the etc2/bc7 formats (create-once by contract), so the 3d residency pool reuses decoded rgba8 textures but not the KTX2 ones a baked world ships, which pay a blocking create per region enter; GL allows compressed storage with per-level glCompressedTexSubImage2D, so the lift is a GL path and a plugin contract, not a design change.
created: 2026-10-20
---

# A block-compressed texture cannot be rewritten in place

## Symptom

The 3d region handler pools geometry buffers and model textures
(okf/plans/3d-world-residency.md): a leave gives them to the package's
resource pool and the next enter takes them back, a write into a retired
id instead of a blocking create. A model's decoded images upload as
mutable textures for that reason. Its block-compressed images (KTX2
transcoded to ETC2 or BC7, what `sol tool 3d/model` bakes and what a
shipped world uses) cannot: the GPU contract says the compressed formats
are create-once and sample-only (`createMutableTexture`, `uploadTexture`,
`resizeTexture` throw for them), so each enter creates them afresh and
each leave destroys them. On a tiled phone GPU that is the create cost
the pool exists to avoid, paid by exactly the textures a production
world has.

## Done looks like

- A mutable texture may take a compressed format: `createMutableTexture`
  with "etc2-*" or "bc7-*" allocates immutable compressed storage with
  the chain, and `uploadTexture` with the full chain re-fills every
  level through `glCompressedTexSubImage2D` (whole-level writes only: a
  rect into a compressed level is block-aligned and stays out of
  contract).
- `resizeTexture` stays refused for them (a new size is a new storage);
  the pool classes by format, size and chain, so a size change is a
  different class.
- The 3d model builder's `takeTexture` then pools them like the rest,
  and the pool test covers a compressed round trip.

## Involves

`alloy/src/gl/` (the texture storage and upload paths), the flux gpu
plugin's format checks, `packages/flux-types/gui/gpu.d.ts` (the
TextureFormat contract paragraph), `packages/3d/src/model.ts`
(`takeTexture`).
