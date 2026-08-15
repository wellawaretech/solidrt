---
title: Location module (geolocation)
description: The runtime exposes camera, microphone, speech-recognition and sound as @solidrt/core subpath modules but has no geolocation API, so apps fall back to a coarse IP lookup over fetch; add flux:location and @solidrt/core/location in the established device-module shape.
created: 2026-08-15
---

# Location module (geolocation)

Some apps want "where am I" as their first question; the workaround, a
coarse IP lookup over `fetch`, is both less accurate and less private than a
platform fix.

## Done looks like

- `flux:location` capability module in flux/src/plugins/modules/, forge
  core underneath, marshalling only in the plugin. Feature availability by
  name via Flux.capabilities, not by OS.
- `@solidrt/core/location` alongside camera/microphone/sound/
  speech-recognition, following the createX reactive-primitive shape
  (`createLocation()` yielding a reactive position plus error(), never a
  throw at construction).
- Standard vocabulary through the SolidRT lens: the web Geolocation
  position shape (`latitude`, `longitude`, `accuracy`, optional altitude/
  heading/speed, timestamp), one-shot `current()` plus a watch stream, no
  PositionOptions ceremony beyond `highAccuracy`.
- Permission is part of the contract: request on first use, surface denied
  as an error() state, same as microphone/camera.

## Involves

- Platform sources: Android via a JNI bridge to LocationManager /
  FusedLocation (same route as the keyboard/text-session bridge; SDL has no
  location API); Linux/desktop via GeoClue2 over D-Bus where present,
  otherwise unavailable-by-capability; macOS/Windows deferred (declare
  unavailable), no IP-lookup fallback in core (that stays app policy).
- packages/flux-types + docs, and the core module docs.
- Stage 1 (bare minimum): the module surface plus one real backend
  (Android), everything else reports unavailable. Desktop backends are
  follow-ups.
