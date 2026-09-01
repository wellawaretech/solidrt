---
title: Point light shadows
description: A PointLight lights but cannot cast - its map is a cube (six faces, samplerCube by the light-to-fragment vector against a stored distance), and the engine has no cube-map texture or target (gpu-cube-maps is the blocker). The node, the falloff and the spot sibling landed 2026-09-02.
created: 2026-08-27
---

# Point light shadows

Symptom: a bulb inside a room lights every wall but casts no shadow -
`castShadow` exists on DirectionalLight and SpotLight only.

Where this stands (2026-09-02): the light model half of the old
spot-and-point item landed - SpotLight and PointLight nodes, the typed
light list (`uLightType`/`uLightPos`/`uLightParams`, core-driven
position slots), Three's windowed inverse falloff, MAX_LIGHTS = 8 - and
spot shadows landed with it (the directional shadow machinery with a
perspective camera, one atlas slot). What remains is exactly the point
light's map:

- **Point lights cast in every direction**, so the map is a cube map
  (six faces, or a dual-paraboloid approximation) and the lookup
  samples `samplerCube` by the light-to-fragment vector against a
  stored distance. The engine has no cube-map texture or target at
  all: [gpu-cube-maps](gpu-cube-maps.md) is the blocker; its file
  carries the decided shape (render-to-face as a face argument on
  `renderTarget`, distance packed into an rgba8 cube color target - no
  cube depth needed).
- On the library side the shape is six internal views into the cube
  target sharing one caster entry list, a packed-distance depth
  material, and a `samplerCube` distance branch in `SHADOW_LOOKUP`; the
  slot stays one per casting light.

## Done looks like

`<PointLight castShadow>` throws shadows in every direction from a bulb
inside a room, on one map slot (the shadow slot model is unchanged). A
scene that needs a lamp was the trigger for spot; the trigger here is a
bulb between walls.
