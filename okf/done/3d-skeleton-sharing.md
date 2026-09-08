---
title: Shared skeletons - a channel-less rigged piece reads the body's joints
description: Wardrobe pieces ship with a skin but no clips, so a dressed character needed app code copying the body's posed joints every frame; bindSkeleton(body, piece) re-binds the piece's palette rows onto the body's joint nodes (Three's bind, Unity's bones, Godot's shared Skeleton3D), zero per-frame work and no core change.
created: 2026-09-03
completed: 2026-09-08
---

# Shared skeletons

## Symptom

Wardrobe-class assets export WITH a skin but WITHOUT channels - the clips
live on the body model alone - so a piece added beside an animated body
holds its bind pose while the body moves. The app-side fix was real
rediscovery, four traps deep: pieces' joints are the body's under
different capitalisation; name matching is necessary but not sufficient
(pieces truncate the body's parent chain above or skip ancestors in the
middle, and the wrong version LOOKS right for every piece whose tree
happens to match); what worked was per-joint chain composition from the
nearest shared ancestor down, in TRS; run per frame from onFrame through
`getTransform`. Measured after the core evaluator: ~1.0 ms/frame for one
character with 6 pieces / 63 bonds, the largest remaining JS line of an
animated character.

## What was done

Neither tier the item shaped (a library copy, or the same clip retargeted
onto the piece's nodes through a second player table). Both keep a posed
copy of the skeleton per piece; the engines do not. `bindSkeleton(body,
piece)` in `packages/3d/src/skeleton.ts`:

- matches the piece's nodes to the body's by case-insensitive name and
  moves each matched joint's palette row (texture, row, the PIECE's
  inverse bind as the post-multiply) onto the body's joint node, anchor
  the body root. The flush then writes `inverse(bodyWorld) * bodyJoint *
  pieceInverseBind` into the piece's palette - the body's WORLD matrices,
  which is what a bone matrix needs, so the piece's mangled tree is never
  composed and the chain traps do not exist;
- checks each shared joint's inverse bind against the body's own
  (tolerance 1e-3) and throws on a mismatch, so a piece exported against
  another rest pose refuses instead of rendering wrong;
- grafts whatever hangs off a matched piece node and is not itself
  matched (an unmatched joint subtree such as a hat's internal bones, a
  rigid part) under the matched body joint, local transform intact;
- parents the piece under the body at identity (its skinned vertices
  are in the body's model space) and remaps its parts' cull groups to
  the body joints;
- unions the pieces' joint-space influence boxes into the body joints'
  culling boxes (`refreshJointBounds`, which createModel also uses now,
  so a body/legs split united its boxes for the first time too);
- comes off at `piece.dispose()`; a body's dispose disposes its pieces.

The spatial core needed nothing: the palette sink already took per-node
rows with a constant post-multiply and one anchor per texture. Cost is
zero per frame, in JS and in core (the rows were being written anyway).

## Evidence

`probes/skeleton-share-probe.tsx`: a synthetic body (Root_0 > Spine_0 >
Spine_1 > Neck > Head), a hat whose table starts at spine_1 with the
ancestors folded in plus an unmatched hat_jnt and a rigid plume, and a
cape that skips spine_0 - all lowercase. Posed, the body's upper part,
the hat's and the cape's render pixel-identical (0 of 921600 pixels
differ, tolerance 0); the grafted brim and plume match hand-socketed
references on the body's Head within 2/255; the shifted-bind piece
throws; disposing the hat leaves the cape identical and the body's rows
at their own count. The heroes-v2 demo migration (delete
`bindRigs`/`poseRigs`, one `bindSkeleton` per worn piece) is the
consumer's side and closes its feedback items 11 and 15.
