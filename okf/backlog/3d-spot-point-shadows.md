---
title: Spot and point light shadows
description: Only DirectionalLight casts. A spot light would be the same shadow view with a perspective camera; a point light needs a cube-map target and six faces, which the engine has no path for (gpu-cube-maps). Demand-gated - no spot or point light NODE exists yet either.
created: 2026-08-27
---

# Spot and point light shadows

Symptom: a lamp, a torch or a headlight cannot cast a shadow, because
`castShadow` is a DirectionalLight option and the scene has no other
light type. Shadow maps landed directional-only in
[3d-shadow-maps](../done/3d-shadow-maps.md); the light list itself is
directional + hemisphere.

Two different sizes of work hide behind one symptom:

- **Spot lights** are the directional machinery with one substitution:
  the internal shadow view's camera is a perspective camera at the
  light's position with the cone angle as its fov, and `lightShadow`
  is unchanged (the matrix carries the projection; the perspective
  divide is already in `shadow`). The prerequisite is a spot light
  NODE (position, direction, angle, penumbra, distance falloff) in the
  light list and in `lit`'s loop, which is a lighting-model addition
  before it is a shadow one.
- **Point lights** cast in every direction, so the map is a cube map
  (six faces, or a dual-paraboloid approximation) and the lookup
  samples `samplerCube` by the light-to-fragment vector against a
  stored distance. The engine has no cube-map texture or target at
  all: [gpu-cube-maps](gpu-cube-maps.md) is the blocker, and the
  point light node is again a lighting-model prerequisite.

## Done looks like

`<SpotLight castShadow>` throws a cone-shaped shadow from a lamp above a
table; `<PointLight castShadow>` throws shadows in every direction from
a bulb inside a room, on one map slot each (the shadow slot stays "one
per casting light"). The demand evidence today is one sun and three
directional lights; a scene that needs a lamp is the trigger.
