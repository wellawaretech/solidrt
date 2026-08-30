---
title: Anisotropic texture filtering
description: Sampling is filter/wrap/mipmap only, so a textured ground plane at a grazing angle - a road ahead of the camera - smears into a mip blur that trilinear filtering cannot fix; EXT_texture_filter_anisotropic is on practically every ES 3.0 device and is one sampler parameter.
created: 2026-08-30
---

# Anisotropic texture filtering

## Symptom

`SamplerState` (`alloy/src/gpu/texture.rs`) and the `SamplerOptions` on
every create* helper carry `filter`, `wrap` and `mipmap`. That is exactly
the trilinear tier, and trilinear picks one mip level per fragment from
the LARGER of the two screen-space derivatives. A surface seen at a
grazing angle - the road ahead of a kart, a floor receding to the
horizon, any track surface from the driver's view - has derivatives that
differ by 8x or more, so the far half of the road blurs into the mip the
long axis chose. `mipmap: true` (which the 3d package tells apps to set
on every surface seen at distance) is what CAUSES the smear; without it
the same road aliases instead.

Anisotropic filtering samples along the long axis and is the standard
answer; every engine defaults to it on. `EXT_texture_filter_anisotropic`
is an extension, not core, at every GL level, but it is present on
essentially every ES 3.0 device (Adreno, Mali, PowerVR, Apple, desktop)
and ANGLE exposes it over D3D11 and Metal. Absence must be a
`limits`-reported fact (max anisotropy 1) and a silent clamp, never an
error, per the extension-probing model.

## Shape

- Engine: `anisotropy?: number` in `SamplerState`, applied as
  `TEXTURE_MAX_ANISOTROPY_EXT` when the extension is present, clamped to
  `MAX_TEXTURE_MAX_ANISOTROPY_EXT`; reported in `limits.maxAnisotropy`.
  One more field in the sampler cache key. Requires `mipmap: true` to do
  anything, which the doc comment says.
- JS: `anisotropy` beside `filter`/`wrap`/`mipmap` on `SamplerOptions`,
  creation-time state that follows the id like the rest (the sampler
  override on `<texture>` may take it too - decide with the override's
  own rule). `flux-types` parity.
- `@solidrt/3d`: `createModel` uploads with a sensible default (4 is the
  usual engine default; a `DEFAULT_ANISOTROPY` constant), and the AGENTS.md
  "any map seen at distance wants mipmap" line gains "and anisotropy".

## Done looks like

A `wrap: "repeat"` road texture on a plane viewed from a low camera stays
sharp to the horizon at `anisotropy: 8`, compared against the same scene
at 1 in a snapshot pair; `limits` reports the device maximum; a device
without the extension renders as today.
