---
type: backlog-item
title: Snapshot diff helper
description: A numeric pixel-delta mode on get_snapshot against the previous capture of the same node, so "does it still render the same" is one call with a number instead of two images an agent has to eyeball.
status: deferred
timestamp: 2026-08-07T00:00:00Z
---

# Snapshot diff helper

Split out of [[mcp-input-injection]] when `send_input` landed (2026-08-07):
the injection half shipped, this companion did not.

The ask: `get_snapshot` compared against the previous capture of the same
node - mean/max pixel delta plus a coarse changed-region grid (like doom's
old "p" tool) - turning "does it still render the same after my change"
into one call with a numeric answer. Models routinely miss few-pixel
regressions when eyeballing two downscaled captures; the original motivation
(two broken arrowheads shipping past every composition check) is the same
one that produced snapshot crop+scale.

Design note from the split, why it is not "cheap": the CLI has no PNG codec
(established when snapshot crop went runtime-side, see
[[mcp-verification-surface]]), so diffing means the RUNTIME retains the
last raw RGBA per (client, node) and answers the diff itself. Open
questions: retention policy (one capture per node is window-sized on the
window node), invalidation (reload, resize, node id churn), and whether the
diff should run before or after crop/scale. Pairs naturally with
`send_input`: interact, then ask "what changed on screen".
