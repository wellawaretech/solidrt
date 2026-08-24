---
title: Route spatial transition settles to package-level handles
description: The "spatialTransitionEnd" engine event carries the raw core NodeId, so a 2d/3d app wanting an "arrived" callback per sprite or mesh must keep its own node-to-handle map (the packages' maps are private). The element transitions route settles to the target's onTransitionEnd handler; the packages should offer the same - an onTransitionEnd per sprite handle / SceneNode - the first thing anyone chaining animations will reach for.
created: 2026-08-24
---

# Route spatial transition settles to package-level handles

## Symptom

A settled node transition emits one `"spatialTransitionEnd"` engine event
with `{ node, component }`, where `node` is the CORE spatial NodeId
([done](../done/spatial-node-transitions.md)). That is the right currency
for the flux layer, but apps hold sprites and SceneNodes, not core ids:
chaining animations ("when it arrives, remove it" / "spring back after
the bounce") means subscribing to the raw event and maintaining a
node-to-handle map by hand, duplicating maps the packages already keep
privately (the 2d layer's `byNode`, the 3d scene's node list).

The element transitions solved this shape already: settles are routed to
the target element's `onTransitionEnd` handler, target-only, no
bubbling.

## What done looks like

The same affordance one level down, subscribed lazily (no event work for
apps that never ask): an `onTransitionEnd` callback on the 2d sprite
handle (and `<Sprite>` prop) and on the 3d SceneNode, receiving the
component name. One shared subscription per package routes through the
existing private maps; record sprites (no node) simply never fire. The
raw engine event stays for direct flux:spatial consumers.
