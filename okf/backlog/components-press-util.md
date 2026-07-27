---
type: backlog-item
title: Press util; end the Pressable exception
description: Press semantics extracted from Pressable into a components-package util; widened to gesture recognizers and promoted to okf/plans/component-gestures.md, this file is a pointer.
status: promoted
timestamp: 2026-07-23T00:00:00Z
---

# Press util; end the Pressable exception

Promoted to okf/plans/component-gestures.md (2026-07-23) after the
scope-widening session: press became the first member of a gesture
recognizer family with an innermost-wins arena, and the staging plus
decisions (placement, byte-identical extraction, no composition
algebra) now live in the plan. This file stays as a pointer.

Original analysis (the three jobs Pressable bundles, the rejected
core-placement options, the press.ts direction) is folded into the
plan's stages 1-2 unchanged.

Related, landed 2026-07-23: the shared option shape moved from
select.tsx to types.ts as `Option` (used by Select and
SegmentedControl), establishing the "shared shapes go through shared
modules, never a sibling import - even type-only" precedent the plan
extends to behavior.
