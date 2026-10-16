---
title: What contact-hardening soft shadows found
description: Facts the PCSS work surfaced that hold without it - a sized light's near-hard contact, the texel cap's resolution dependence, light layers and shadow views, one depth texture on two units on Adreno 610, capped loops unrolling on Adreno, heat drift on the tablet, one kernel instantiation per program, device costs per tier (SM-T500, Pixel 7, TV), the cascade band's continuity, texel-width floors leaking at contacts on coarse maps, the first-person stair, the frames command for still-scene timings, and the adb-drains-stdin bench trap.
created: 2026-10-16
---

# What contact-hardening soft shadows found

Cut from [3d-contact-hardening-shadows](../done/3d-contact-hardening-shadows.md)
at its close.


- **The first-person example shows the near cascade's texel stair on
  the pillar shadows, on every client (2026-10-16).** The user saw jagged
  edges on the nearest and the leftmost pillar's shadow on the tablet;
  the desktop draws the same. A probe with the scene's shadow knobs on a
  debug command (deleted after) placed it: the stair's pitch halves with
  mapSize 4096 and grows with one box map, and it is identical under the
  one hardware tap, the hard tier and the radius-2 disc at medium, so it
  is the aliasing of a shadow edge that runs nearly along the map's
  texel grid (a pillar's side under a sun at that azimuth: long runs per
  texel step), which a kernel of radius 2 (the old 3x3 grid reached the
  same +-2 texels) rounds but cannot hide. What hides it: resolution
  (4096), a radius of 4 at the high tier (wavy at medium), or an
  `angularSize` of about 2 degrees (the real sun's 0.53 changes nothing
  at 2048 texels over the courtyard: a 3 m caster's tip penumbra is
  under two texels). Open: whether the example should set a size, a
  radius or a map size; the kernel is not at fault.
- **A point light's face maps give the same pair, and a cascade band
  reads the same width as either cascade alone (2026-10-16).** The
  example gained a lamp (`sourceRadius`, six face maps) over a pad on its
  own layer bit and a `split` on the sun; the test reads the lamp's foot
  0.14 / tip 0.48 at medium, and a row across the wall's shadow placed in
  cascade 0's blend band (the layout replays the scene's cascade fit from
  `@solidrt/3d/math` and the test bisects the split) measures 0.22 in
  the band, 0.22 under cascade 0 alone and 0.22 under cascade 1 alone.
  The lamp's six maps plus a second cascade plus the spot overflow the
  8-slot shadow budget, so the cascade reads turn the lamp's shadow off
  (`lamp({ castShadow: false })`); the overflow throws, as it should.
- **Cost on the Pixel 7 (Mali-G710), release client, 1080x2400 portrait,
  GPU ms per frame over 200 consecutive frames (`frames({ count })`),
  skin 25 C, clock sampled at 762 to 848 MHz during each row:**

  | configuration | ms |
  | --- | --- |
  | hard tier | 8.5 |
  | no size, medium tier | 9.3 |
  | low (8 / 8) | 12.7 |
  | medium (16 / 16, default) | 15.5 to 15.8 |
  | high (24 / 32) | 18.7 |

  The soft path costs about 0.4 ms per tap over the frame's ground here;
  the medium tier fits under the 11 ms period only with the hard tier,
  so a phone app at 90 Hz still picks low or hard for a full-screen
  receiver.
- **Cost on the TV (Mali-T860, Android 8), release client, 1920x1080,
  GPU ms per frame, 30 nudged frames per row after a 60 s idle (no
  thermal zone is readable there), rows repeat within 2 ms:**

  | configuration | ms |
  | --- | --- |
  | hard tier, or no size | 348 to 351 |
  | low (8 / 8) | 480 |
  | medium (16 / 16, default) | 545 to 546 |
  | high (24 / 32) | 621 |

  Three frames a second at the hard tier already: this GPU is below
  the 3d baseline for a full-screen shadowed scene at 1080p whatever the
  tier, and the soft tiers add 37, 56 and 78 percent on top. The number
  that matters for the design is the ratio, which matches the tablet's.
- **A still scene is timed with `frames({ count })` now, not a nudge
  loop (2026-10-16).** A debug call per frame from the host leaves a
  Mali governor at 250 to 500 MHz (the Pixel's rows read 20 ms at hard
  that way, 8.5 at 848 MHz); the example's `frames` command registers an
  `onFrame` subscriber that nudges the sun's intensity each frame and
  cancels itself after the count, so the device draws them back to back
  and the governor ramps. The control-API form of this stays in tiny.md.
- **The empty `/stats` bodies were the bench script, not the server
  (2026-10-16).** The 2026-10-15 cooldown script (and this session's
  first) passed `"$(temp)"`, an adb call inside a command substitution,
  in the argument list of the python that read the stats reply from a
  pipe; adb drains its stdin, so the pipe arrived empty. Read every
  value before the pipe; the ideas.md entry is withdrawn.
- **A sized light's contact is near-hard, not hard.** The blocker search
  averages every occluder in its region, so under a tall caster it
  includes the caster's side up to the search radius and the foot's
  penumbra is about the light's size times the height the search
  reaches, not zero. Every PCSS behaves so; the headless test bounds a
  sized foot at 0.25 world units and a point source's at 0.15.
  Measured on the example at medium (transition width across the edge,
  world units, foot / tip): sun box 0.04 / 0.38, sun as two cascades
  0.10 / 0.34, spot 0.14 / 0.34; size 0 or the hard tier 0.02 to 0.04
  everywhere.
- **The texel cap makes softness a function of map resolution.** With
  `SHADOW_MAX_TEXELS` 8, a finer map caps the penumbra lower in world
  units; the example uses coarse maps (384 and 256) so the tips read.
  The cap is deliberately not tied to the quality tier: a tier changes
  the grain, never the look. If a scene wants wider penumbrae than its
  map allows, the knob is `mapSize` (or a tighter frustum), and the
  device numbers decide whether the cap itself should rise.
- **Light layers and shadow views.** A casting light's shadow view draws
  the meshes the SCENE draws that the light lights (its mask is the
  scene mask AND the light's layers), so a light on a bit the scene's
  `layers` does not hold casts nothing. The example keeps the post out
  of the sun this way and needed `layers={3}` on the Scene for it.
- **Adreno 610 takes one depth texture on two units, with and without
  compare, in one draw.** The SM-T500 (release client of 2026-10-15)
  renders the example exactly as the desktop does: hard foot, wide tip,
  the spot's post the same; no driver or shader warning. The feared
  R32F-copy fallback is not needed.
- **A capped loop bound unrolls into a fixed cost.** The first kernel
  looped to MAX_SHADOW_TAPS with a break on the tier's count, and spent
  a sine, cosine and root per tap; on the tablet the three tiers cost
  the same (about 2.5x a hard frame): the compiler unrolled both loops
  to the table's length and predicated every iteration. The disc is now
  a constant table rotated by one matrix per kernel, and the loops run to
  the tap-count uniform itself with the table index masked, so the
  compiler has no cap to unroll to. Costs then scale with the tier.
- **Tablet measurements drift with heat and the clock is unreadable.**
  Every kgsl clock node is permission-denied on the SM-T500, and the
  same build measured 240 and 335 ms at medium minutes apart as the GPU
  sensor climbed from 33 to 40 C. Rows are only comparable when taken
  after a cooldown (the `cool.sh` protocol: wait for gpu-usr under 35 C
  before each configuration) or within one short batch.
- **The kernel must be instantiated ONCE in the fragment program.** The
  lookup first called `shadowSample` at three sites (a point light's
  face, the cascade, its blend partner); inlined three times the
  program outgrew the Adreno 610 and every pixel paid, the hard path
  included (115 ms a frame against 72 with one copy; the soft tiers two
  and a half times their cost). Three one-site forms were tried on the
  tablet before the one that works: a two-pass loop over the chosen
  maps with dynamically indexed local arrays (scratch memory, hard 286
  ms), a counted two-pass loop over scalars (unrolled into two
  predicated copies, hard 105, medium 190), a while loop on the map
  index with lean state (hard 105 still). The form that keeps the gain
  is the original scan itself: ONE `for` over the light's maps with the
  blend partner sampled by the same call on the next iteration and a
  `blend` sentinel saying so (hard 89, low 123, medium 135, each after
  a fixed idle). Rule for every future kernel of this size: one textual
  call, inside the scan loop, scalars only, no counted loop around it.
- **Cost on the SM-T500 (Adreno 610), release client, 2000x1092, the
  example's two casters, GPU ms per frame from the compositor
  timestamps, each row after a fixed idle of 100 s (the USB link and
  with it the thermal sensor had dropped; the rows repeat within 3 ms):**

  | configuration | ms |
  | --- | --- |
  | hard tier, or no size and radius 1 | 77 to 79 |
  | low (8 / 8) | 117 |
  | medium (16 / 16, default) | 139 to 141 |
  | high (24 / 32) | 189 |

  The soft path costs its taps and nothing else now: about 2.5 ms per
  tap over a ground that fills the frame. A default sun size stays 0:
  on this class of device the medium tier is a third of the frame, the
  app's `shadows.quality` and the per-light sizes decide.
- **A texel-based filter width leaks at contacts on a coarse map
  (2026-10-16).** With the tier's 2.5 texels as the floor, the
  soft-shadows example's wall foot read 0.12 world units on the 3.65 cm
  box map, about 0.2 on a 4.3 cm cascade and 0.7 with noise inside the
  umbra on the 13 cm second cascade: the taps of a kernel wider than the
  contact's depth gap reach past the occluder (the wall's side a few
  centimetres above the receiver) and read lit, and the receiver-plane
  slope bias widens it further (0.28 to 0.40 on the 4.3 cm map with the
  slope off and on). Away from the contact the tip reads the same on
  every map (0.14 to 0.16). Unity's tents on a coarse map do the same;
  its normal bias is texel-scaled, ours is in world units. The width a
  tier asks for is meant for maps whose texel is a centimetre or less;
  an example with coarse maps pins radius 1.
- **The first-person pillars' stair is texel aliasing, hidden by the
  tier's width, not by the kernel (2026-10-16).** See the done record;
  the probe placed it: pitch halves with 4096 maps, grows with one box
  map, identical under the one tap, the hard tier and the radius-2 disc.
  Unity's Medium tent (5x5) is what hides it there; radius 2.5 is the
  equivalent here.
