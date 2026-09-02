---
title: Shared skeletons - drive a channel-less rigged piece from a body's clips
description: Cosmetic and attachment models ship with a skin but zero animation channels (clips live on the body), so a dressed character needs app code copying the body's posed joints across every frame; give the library a bindSkeleton/drives path built on the evaluator's target tables, where retargeting a clip is handing it the piece's nodes.
created: 2026-09-03
---

# Shared skeletons

## Symptom

Wardrobe-class assets export WITH a skin but WITHOUT channels - the clips
live on the body model alone - so a piece added beside an animated body
holds its bind pose while the body moves. Since a dressed character is
the normal case for the asset class, "the body animates" is not a working
result on its own. The app-side fix is real rediscovery, four traps deep:

- pieces' joints are the body's under different capitalisation, so the
  match is by case-insensitive name;
- name matching is NECESSARY BUT NOT SUFFICIENT: pieces do not reliably
  reproduce the body's parent chain (tables truncated above, ancestors
  skipped in the middle), and the wrong version LOOKS right for every
  piece whose tree happens to match;
- what works is per-joint chain composition from the nearest SHARED
  ancestor down, in TRS (the reference implementation is a demo's
  `bindRigs`/`poseRigs`);
- the copy runs per frame from onFrame and reads poses through
  `getTransform` (animated joints' JS mirrors are stale).

Measured cost after the core evaluator: ~1.0 ms/frame for one character
with 6 pieces / 63 bonds - the largest remaining JS line of an animated
character, all of it this copy.

## Shape

Two tiers, cheapest first:

1. Library `bindSkeleton(body, piece)` returning the per-joint bonds
   (name match + shared-ancestor chains), plus a mixer option
   (`drives: [pieces]`) that runs the copy - the demo's code moved into
   packages/3d, so apps stop rediscovering it. Still O(bonds) JS per
   frame.
2. Native: the evaluator's clips already target INTEGER SLOTS bound per
   player to a NodeId table ([animation-core](../done/animation-core.md)
   reserved this seam) - drive a piece by giving the SAME clip a second
   player whose table maps the body's slots onto the piece's nodes
   (identity-chain joints only; chain-composed bonds stay in tier 1 or
   get a core answer only if the JS line still shows). Retargeting is a
   table, no name or model concept enters core.

## Done looks like

`createMixer(body, { drives: pieces })` (name against Unity/Godot/Three
conventions before settling) animates a dressed character with no
app-side skeleton code; the demo deletes `bindRigs`/`poseRigs` and its
remaining animation JS reads ~0. Pairs with
[3d-root-motion](3d-root-motion.md).
