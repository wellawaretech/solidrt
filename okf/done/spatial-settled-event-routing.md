---
title: Route spatial transition settles to package-level handles
description: The "spatialTransitionEnd" engine event carries the raw core NodeId, so a 2d/3d app wanting an "arrived" callback per sprite or mesh must keep its own node-to-handle map (the packages' maps are private). The element transitions route settles to the target's onTransitionEnd handler; the packages should offer the same - an onTransitionEnd per sprite handle / SceneNode - the first thing anyone chaining animations will reach for.
created: 2026-08-24
completed: 2026-08-24
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

## Findings

Landed 2026-08-24 (uncommitted), package-side only - `flux:spatial` and
the engine event are untouched.

- `@solidrt/2d`: `onTransitionEnd?: (event: { component }) => void` is a
  plain assignable field on `Sprite` and `SpriteGroup` (the pointer-handler
  rule: touches no GPU state), and a prop on `<Sprite>`/`<Group>`. A
  module-level `Map<NodeId, Sprite | SpriteGroup>` indexes only DECLARED
  handles: `setSpriteTransition`/`setGroupTransition` register (a null
  config unregisters), remove/dispose unregister; `addSprite` is
  untouched, so an app that never declares pays nothing. One lazy
  `on("spatialTransitionEnd")` subscription from `srt:events` (its ambient
  declaration reaches the packages through core's `types.d.ts` reference)
  starts at the first declaration and is never torn down; misses for
  unregistered nodes are a map lookup. Handler errors are caught and
  logged like core's element `onTransitionEnd`.
- `@solidrt/3d`: the same field on `SceneNode`; the registry keys the
  core id while the node is in a scene (`enterScene` registers when a
  declaration exists, `setTransition` registers/unregisters in place,
  `leaveScene` unregisters). The 3d components carry no `transition`
  prop (function face only), so no `onTransitionEnd` prop there either.
- Payload key is `component` ("position" | "rotation" | "scale"), the
  vocabulary the engine event and the docs already use, not the element
  event's `property`.

Verified live (release go client, srt run + control-API logs). 3d probe:
a mesh with a 300 ms position spring and a 200 ms rotation tween settled
rotation then position with the landing exact (x = 1.000); clearing the
declaration inside the handler and writing again snapped with no further
settle. 2d function-face probe: a sprite (position + rotation) and a
group (scale) settled in duration order; a sprite whose declaration was
cleared mid-flight and one removed mid-flight never fired.

Not verified: the `<Sprite onTransitionEnd>` prop, because the component
face is broken independently of this item - the unchanged
`packages/2d/examples/pick.tsx` halts at mount with "Context must either
be created with a default value or a value must be provided before
accessing it" from `useContext(LayerContext)` inside `<Sprite>`. Filed as
[2d-sprite-layer-context-halt](../backlog/2d-sprite-layer-context-halt.md).
