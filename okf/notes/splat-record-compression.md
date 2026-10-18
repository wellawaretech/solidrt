---
title: Splat record compression measurements and the 11-bit lesson
description: What building the chunked splat record established - importance banding costs 12x in position precision, the field's 11-bit position fails a capture's micro-splats where 16 bits pass a human flip, the decode and chunk fetches cost about 1.5 ms per million splats on Mali, the packed sort is a wash on the phone and 25% slower on a laptop, and the snapshot and phone protocols that measured it.
created: 2026-10-18
---

# Splat record compression measurements and the 11-bit lesson

Cut from [../done/splat-record-compression.md](../done/splat-record-compression.md)
at its close. Each bullet is dated by the work it came out of.


- Step 1 (the format and bake) built 2026-10-18. The tool bakes the 1M
  train in 0.6 s and the 786k lizard at SH3 in 1.0 s under bun; the
  files are 15.8 and 48.1 MiB (plus the 4-byte order block: 19.7 and
  51.1) against the version 2 bakes' 32.8 and 100.6.
- The first design ordered the records by importance band (one octave
  each, at least 16 chunks) and k-d chunked within a band, so a count
  prefix needed no order block. Measured on the train through the
  reference decoder: 35 bands, chunk extent p50 4.9 units, the chunk
  step over the splat's largest axis p50 20%, p99 5x, and 9% of splats
  moved by more than their own size - every band spreads over the
  whole scene, so a chunk of 256 same-band splats is a slab, not a box.
  One band (pure spatial k-d): extent p50 0.29, step p50 1.6%, p99 36%,
  0.19% of splats over their size. 12x at the median; the bands went
  and the draw order moved into its own block.
- What remains after that is the fixed chunk size meeting sparse
  regions: the lizard (bounds 400 units from a handful of floaters at
  the box's edges) has 1% of chunks wider than 170 units and 1.7% of
  splats moved by more than their largest axis, all in near-empty space.
  A k-d leaf holds 256 splats whatever their spread; only a variable
  chunk size (padding sparse leaves, which the order block makes
  drawable-free) or more position bits would change it. Judged by the
  side-by-side in step 4 before deciding anything.
- A 10-bit unorm over a symmetric range puts zero at 511.5, so an
  axis-aligned splat's rotation could not decode exactly; the stored
  components span 1022 steps (zero at 511, one unused code) so the
  common case is exact.
- Steps 2 and 3 built 2026-10-18: the packed key in alloy's order module
  (KeyColumn.packed, PackedKey, PackedRecords, order_permutation_packed,
  materialize_indices_mapped, the sort job carrying the chunk table), the
  flux marshalling and flux-types, the 3d mesh's `packed` + `order` pass-
  through, the splat runtime's vertex decode over the chunk texture, the
  alpha cap and the antialiased compensation. 16 bun, 785 alloy (4 new),
  83 flux gui tests and the splat app test green on the rebuilt client.
- The entry's InstanceOrder still checks that a 12-byte position key fits
  the record stride (it does not know the key is packed), so a packed
  word must sit at least 12 bytes before the record end. Ours is at
  offset 0; a third-party layout with the word last would be refused at
  attach with "does not fit". A known limit, not widened.
- Snapshot diff, laptop, 1707x960, default pose, linear blend, version 2
  (HEAD's client from a worktree) against version 3: train mean 0.17/255,
  max 28, 77% of pixels exact; lizard SH3 mean 0.27/255, max 119 in 264
  of 2.9M pixels, 81% exact. The sRGB acceptance pair's residue was
  1.2-1.4/255. Both pairs are in scratch/captures for the human verdict.
- Laptop A/B the same warm afternoon (both clients orbiting the 1M train,
  alternately): scene pass 203 ms/frame on version 2, 200 on version 3 -
  the decode, the chunk fetches and the covariance build cost nothing on
  the Intel iGPU, as stage C predicted. The sort on the thread: 53-65 ms
  before, 68-79 after. The key pass reads 8 bytes per record instead of
  12, but the mapped materialization gathers each record id through the
  permutation (1M random 4-byte reads), which the slot form never did.
  The fix if the phone confirms it: carry (key, id) as one u64 through
  the radix passes, which also deletes the keys[slot] random read every
  pass does today, and take the ids off the low words at the end.
- Pixel 7, 2026-10-18, the new arm64 client, cold (skin 25-26 C, status
  0), cadence hold off, every row at 848 MHz on all five samples, the 1M
  train, portrait 1079x2399, orbit at 0.3 rad/s, 10 s windows
  (`gpuFrameExecMsPerFrame`; this Mali's pass timers read 0):

  | rung | now | stage D record |
  |---|---|---|
  | 1M half-res noraster (the vertex side) | 9.0 | 7.4-8.0 |
  | 1M full-res noraster | 8.5 | - |
  | 1M full-res stock, display blend | 44.6 | 40.9 |
  | 1M full-res stock, linear blend | 88.1 | 35.8-42.0 |
  | 1M half-res stock, linear | 30.2 | - |
  | 300k full-res stock, linear | 63.3 | 23.5 |
  | sort on the thread at 1M, last job ms | 37-56 | 47-53 |
  | native heap, 1M mounted (dumpsys), first cut / revised | 75-78 / 113 MB | 113 MB |

  The vertex side with the one-texel decode, the three chunk fetches and
  the covariance build is within a millisecond of the record: the
  format costs nothing per vertex on Mali either. The sort on the thread
  is at or under the record's (the packed key streams 8 bytes per record
  sequentially; the mapped id gather that cost the laptop 25% does not
  show on the phone), so the u64 radix idea stays an idea. The first
  cut's 8-byte mirror took the native heap to 75-78 MB; the revised
  12-byte mirror reads 113 MB, the record's figure.
- The linear-blend rows are not this item's: the display-blend row sits
  within session drift of the record's, so the splat fill is unchanged,
  while linear reads twice display where the record had them equal (the
  2026-09-27 A/B: 41.96 against 40.94). Something between then and HEAD
  doubled the half-float scene path on this GPU. Confirmed the same
  session with HEAD's own client (a worktree build, the version 2 bakes
  from the worktree's server), same protocol, every row at 848 MHz:

  | rung | HEAD | this build |
  |---|---|---|
  | 1M full-res stock, linear | 88.1 | 88.1 |
  | 1M full-res stock, display | 43.7 | 44.6 |
  | 1M half-res noraster | 7.6 (701-848 MHz) | 9.0 |
  | sort on the thread, last job ms | 36-50 | 37-56 |

  The fill is identical, the decode and the three chunk fetches cost
  about 1.5 ms per million splats on the vertex side (2% of the frame),
  the sort is a wash. The linear-blend doubling is HEAD's:
  okf/backlog/mali-linear-scene-path-doubled.md.
- The human verdict on the 11/10/11 pairs: the train indistinguishable,
  the lizard visibly worse on the right side of its head. The diff
  localizes it to a mottled patch of cheek behind the eye, and the bake
  explains it: chunk 1346 sits on the head, is 2.2 units wide and holds
  256 splats under 0.02 units, the finest 0.0002 - metal scales made of
  micro-splats, moved by up to 5.6 times their size at an 11-bit step.
  Not floaters: a dense region whose splats are finer than 1/2047 of a
  head-sized box. The field's record (PlayCanvas, Unity) fails this
  capture the same way; sogs went to 16-bit means for this reason, and
  the lizard's own spz source quantizes at 1/4096 units, itself marginal
  against these splats. Revised to 16 bits per axis in two words (65536
  steps: the lizard's worst splat moves 0.7 of its size, the 99th
  percentile 9%; the train's 99th percentile 1.1%), the color moved to
  an rgba8 block and texture, 16 bits reserved. 20 bytes per splat on
  the GPU and a 12-byte mirror: a third off version 2 instead of half.
- The revised bakes: train 23.7 MiB (step over largest axis median
  0.04%, p99 0.76%, 0.01% of splats above a tenth), lizard SH3 54.1 MiB
  (median 0.07%, p99 7.7%, 0.9% above a tenth - the remaining tail is
  the sparse-region chunks). Snapshot diff against version 2: train mean
  0.105/255 (was 0.17), lizard 0.033/255 (was 0.27), the head region's
  max 38 (was 122) with the mottled patch gone in the crop. Pixel 7, same
  protocol: vertex side 8.4 ms (701-848 MHz), display-blend 1M full-res
  44.5 (HEAD 43.7), sort 41 ms; the second word and the color fetch cost
  nothing measurable over the first cut. All suites green again (16 bun,
  785 alloy, 83 flux gui, the app test).
