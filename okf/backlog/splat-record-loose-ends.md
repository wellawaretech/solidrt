---
title: The splat record's loose ends - an order check that does not know the key is packed, two boundaries without tests, 16 dead bits, a constant scale floor, an unjudged antialiased path
description: Five small debts left by okf/done/splat-record-compression.md, fixed together - the entry's instanceOrder still demands 12 bytes from a packed key's offset; the flux `packed` object shape and the 3d `packed`/`order` validation have no tests; word 1's high 16 bits carry nothing; the log-scale clamp is a constant a kilometre-scale capture would clip; and the Mip-Splatting compensation has never been looked at on a capture trained with it.
created: 2026-10-18
---

# The splat record's loose ends

## Symptom

- `InstanceOrder::parse` runs `check_stride` on an indexed order and a
  position key reads 12 bytes, so a third-party record with the packed
  position words in its last 8 bytes is refused at attach with "does
  not fit", although the hand-off's own `KeyColumn::parse` already
  checked the packed words against the stride. Ours sits at offset 0,
  which is why nothing fails today.
- `flux/src/alloy_plugins/gpu.rs` parses `packed: { format, chunk,
  bounds }` and `order` with no flux module test; `resolveInstanceOrder`
  in `packages/3d/src/mesh.ts` rejects a packed order without `order`,
  a non-`uint32x2` position attribute and a mismatched order length
  with no test of those throws. The core's `check_packed` is tested.
- Word 1 of the record is z in the low half and zero in the high half:
  2 bytes per splat, 2 MB at 1M, reserved for nothing named.
- `SPLAT_LOG_SCALE_MIN = -12` / `MAX = 10` are constants; a capture in
  kilometres with millimetre splats (log -13.8) clips to the floor.
- The `antialiased` header flag and the compensation `sqrt(det(S) /
  det(S + dilation))` are covered by the flag's round trip and the
  shader variant compiling; neither test capture was trained in that
  mode, so no human has seen the path do its job.

## Shape

- Drop `check_stride` for an indexed order (the attach check and
  `KeyColumn::accepts` decide against the hand-off), or let the entry's
  `InstanceOrder` carry `packed` so the check knows the key's width.
  Test: a 16-byte record with the packed words at float 2 attaches.
- A `flux/tests/gpu.test.ts` case for `transferRecords` with the packed
  key (shape errors by name, a valid hand-off accepted) and bun tests in
  `packages/3d/tests` for the three `resolveInstanceOrder` throws and
  `createSplatMesh`'s order and color block checks.
- The 16 bits: decided in okf/plans/splat-order-and-chunks.md - they
  carry the record's chunk index once chunks are variable runs.
- Derive the log-scale range from the cloud at bake (its own min and
  max, clamped only against non-finite), and store nothing new: the
  chunk boxes already carry the range.
- Find or train a capture in nerfstudio's `antialiased` mode (or
  Mip-Splatting's), bake it with and without `--antialiased`, judge the
  pair against the trainer's own render.
