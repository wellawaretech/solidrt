---
type: backlog-item
title: A zoom debug command in the scaffold
description: Snapshots reach an agent downscaled, so small hand-authored geometry needs magnified inspection; a ~15-line viewBox-shrinking registerDebug("zoom") turns "look closely at X" into one call, worth shipping in the create-solidrt scaffold.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# A zoom debug command in the scaffold

Source: the animated-explainer demo feedback (2026-08-02). Snapshots are
downscaled by the time a model sees them, so a full-window capture cannot
show a defect a few pixels across - two broken arrowhead constructions
shipped past every composition check and were caught by a human at the
screen. The AGENTS.md guidance ("inspect hand-authored geometry magnified
once") landed 2026-08-02; this item is the tool that makes it one call.

In a viewBox app, zooming the real app is nearly free: shrink the design
box to the region and translate the content so the region starts at the
origin - the scale falls out.

```tsx
<view flex={1} viewBox={[zoom()?.w ?? DESIGN_W, zoom()?.h ?? DESIGN_H]}>
  <d-view x={-(zoom()?.x ?? 0)} y={-(zoom()?.y ?? 0)}>
    ...
  </d-view>
</view>
```

Paired with `registerDebug("zoom", ...)` setting the region, an agent runs
`zoom({x: 580, y: 250, w: 140})` and snapshots at 9x without touching
source. About 15 lines; costs nothing while unused. Note it uses
translation only - a group scale here would hit
[[detached-view-transform-origin]].

Worth considering for the create-solidrt scaffold alongside the app
itself; it pairs naturally with seek/pause debug commands, so an agent can
inspect any moment at any scale.
