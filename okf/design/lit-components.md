---
title: Lit components
description: The light and material model for @solidrt/components - a theme light, materials by role, elevation, and one Surface primitive drawing every control's chrome on the 2D path - with the decisions the prototype settled (no 3D-rendered controls, hover lift over tilt, press sink over press scale, an app-owned light at event rate) and the open items. Read before moving the model into the package.
created: 2026-09-27
---

# Lit components

The component library reads as paper with no light on it next to the 3D
demos: every control is a fill rect, a press tint and a stroke rect. The
difference is not the third dimension but light, depth cues and physical
motion. This document is the model that brings those into the theme while
the drawing stays on the 2D path (partial repaint, repaint boundaries,
demand-driven frames, crisp vector text), and the decisions a prototype
settled. The model is in the package since 2026-09-27 (see "What the package
has"); this document remains the reasoning behind it.

The prototype that settled it lives in the `demo/comp` project beside this
checkout (`src/lit/`, gallery in `src/index.tsx`; its own write-ups are
`LIT-COMPONENTS.md`, `TERMINAL-THEME.md`, `SOLIDRT-FEEDBACK.md` and
`TEXT-CENTERING.md` there); the package's own page is
packages/components/examples/lit.tsx.

## The model

**A light in the theme.** One directional key light in screen space (x
right, y down, z into the screen), a color and an ambient fill 0..1. Every
surface derives its look from it: the drop shadow's offset is the elevation
along (x, y) scaled by 1/z, its blur grows with elevation; the bevel is a
stroke whose gradient runs from the lit edge (mixed toward the light color)
to the shaded edge (toward black); the sheen is a subtle fill gradient along
the same axis. Lit faces also blend toward base x light color (a per-channel
multiply in linear light, `LIGHT_TINT` 0.3 x (1 - ambient)): without it the
light's color is invisible on a light scheme, since a 1px bevel and a sheen
edge carry too little of it. The mapping constants are named constants in
the light module, tuned by eye.

**Materials by role.** A material is a stack of optional layers: `sheen`
and `bevel` strengths, `glass` (a frosted backdrop: blur and tint), `glow`
(a zero-offset colored shadow, the fill's own color when uncolored) and
`border` (a fixed outline under the bevel). `{}` is the flat look. The theme
assigns a material to each of four roles - `surface` (Card, Item, panes),
`control` (secondary Button, TextInput, Select trigger, tracks), `accent`
(primary/danger Button, Switch on, Slider fill), `overlay` (Modal, Tooltip,
dropdowns, menus) - and a component picks a role, never a material. Presets
restyle the whole library: flat, lit, glass (the overlay role frosted), a
dark palette with a glowing accent.

**Elevation.** Four levels, flat 0, raised 2, floating 6, overlay 14 (units
of shadow offset and blur), tuned by eye. Item sits at flat by default: a
list of raised rows is an app's deliberate choice, priced in
[lit-list-cost](../notes/lit-list-cost.md). Two shadows per raised surface: a
directional key shadow and a tight contact shadow (half a pixel of offset per
unit of slant, small blur) where the surface meets the ground. Shadow
strength is 1 on light schemes and 1.7 on dark, where a shadow otherwise
vanishes.

**Surface.** One primitive draws every control's chrome, in this order: key
shadow, contact shadow, glow, fill (base color or the sheen gradient; inside
a backdrop-filtered child view when the material is glass), hover/pressed
tint, bevel, outline (focus ring or border). It renders as the first child of
the laid-out view it dresses; every layer is detached except the glass one,
an absolutely positioned child view with `overflow="hidden"`, `clipRadius`
and `backdropFilter`, so the root needs no filter and never clips and shadows
drawn outside the box survive. A sunken surface (a track, a groove, an
input) inverts bevel and sheen and casts nothing; a pressed face uses the
same inversion.

**A cap color.** The theme's `color.thumb` (Switch knob, Slider thumb):
light in every scheme, lit like any face. This is in the package.

**Motion.** A press sinks rather than shrinks: the key shadow fades to 10%
(the contact shadow that remains reads as touching down), the lit edge moves
to the far side, a spring plays on release; a material with no depth keeps
the 0.97 press scale. A hover lift on cards: the key shadow cross-fades to
the next elevation's, the card rises two whole pixels, a faint light-colored
highlight follows the pointer; nothing rotates or scales, so text stays on
the pixel grid. Staggered entry for lists, menus and popups through the
runtime's `stagger` and `from`. All of it is movement, so `policy.motion`
"reduced" and "none" snap it.

**Living backgrounds and the window finish.** A shader texture or a scene
behind the UI, lit by the theme light; the finish (vignette, scanlines, a
phosphor halo) is `theme.finish`, a compiled window shader Window declares.
No threshold bloom: the UI frame is 8-bit and white text would glow.

## Decisions

- **No controls rendered in 3D.** Text lives in the rendertree (labels,
  TextInput, IME, selection, the editor), a scene is a cleared HDR target
  that gives up partial repaint and demand-driven frames, Impeller's vector
  shapes and text are sharper at 1x than rasterized meshes, and layout, hit
  testing and focus would have to be mirrored into mesh transforms. Real 3D
  belongs in a few opt-in components on @solidrt/3d (a cover flow, a dial, a
  hero object, device-tilt parallax), one scene per screen, in a package on
  top of both. @solidrt/components depends on core only.
- **Pointer-follow tilt rejected, hover lift instead.** At a few degrees
  the 1px edges and text baselines alias into stair steps on a low-DPI
  display, and the shadow tilting with the card reads as a flat cutout.
- **Press sink replaces the press scale** on any material with depth.
- **The app owns the light's direction.** A preset only suggests a color
  and an ambient, applied until the app sets its own, so a user's choice
  survives preset and scheme switches.
- **The light changes at event rate, not per frame.** With about 48 lit
  surfaces each deriving two gradients and two shadows, an orbiting light
  costs ~17 ms of JS per frame straightforwardly and p50 ~11 ms with the
  axis quantized to 128 steps and derived paint interned (194 to ~30 prop
  writes per frame); frames where the axis steps still cost ~9 ms. A drag,
  a preset switch or a time of day that changes every few seconds is fine.
  Per-frame light would need a shared paint value in the runtime (an
  ideas.md line), not more caching.
- **Label polarity reads the unlit base.** `lightOnDark` compares against
  Surface's base color, not the painted gradient, so the low-DPI weight
  compensation keeps working on lit fills.
- **Screen space stays screen space.** The background shader reads the
  screen-space direction directly. A `<Scene>` sharing the light needs
  `[x, -y, -z]` (y flipped, z into the screen becoming the camera's -z);
  derived, not tested.
- **Glass is an option for the overlay role, not a default for surfaces:**
  one panel holds the refresh on every measured device, four drop the
  tablet and the TV to 22 fps
  ([backdrop-filter-cost](../notes/backdrop-filter-cost.md)).
- **Shadows cast from opaque rects.** A transparent caster's blurred
  shadow composites in a save layer of its own; in a list that is 8-19 fps
  on every tiled GPU. Only glass keeps a transparent caster.

## What the package has

Since 2026-09-27 the model is the package's
([lit-components-package](../done/lit-components-package.md)): `Surface`
draws every component's faces, `theme.light`, `theme.shadow`,
`theme.material` (per role, the glow as a layer of it) and
`theme.elevation` describe them, `StyleProps` and `theme.components` take
`material` and `elevation`, and `litDarkTheme`/`litLightTheme` ship beside
the flat presets, which render exactly as before. In core: `shadow`
animates as one value under `transition={{ shadow }}`, to and from none
([shadow-transition](../done/shadow-transition.md)), and a transparent
caster's shadow composites correctly under group opacity, at the cost of a
layer of its own. The terminal hooks landed the same day: `pill()`,
`color.thumb`, borders drawn by every themed component, `theme.finish`
declared by Window.

## Readability under light

The sheen lifts the faces text sits on and Impeller renders light-on-dark
text thin (glyph coverage composites in nonlinear sRGB). On dark palettes the
prototype starts regular text a weight step heavier (500), lifts muted text
to about 7:1 or better on every surface, and keeps text at 13px or larger.
The window finish must not crush contrast: no contrast grade, a vignette
capped so corners keep at least 85% of their light, one strength (default
about 40%) scaling every layer, and a phosphor halo that adds light only
where the surroundings are dark.

## Known limits and open items

- Measured 2026-09-27 ([lit-list-cost](../notes/lit-list-cost.md),
  [backdrop-filter-cost](../notes/backdrop-filter-cost.md)): two blurred
  shadows per row roughly halve the tablet and the TV in a scrolling list, a
  transparent shadow caster (a save layer per shadow) is unusable there,
  and one glass panel is fine everywhere while four are not on the tablet
  or the TV. So lit is a preset, Item is flat by default, Surface casts
  from opaque rects, and glass stays the overlay role's option
  ([lit-components-package](../done/lit-components-package.md)).
- A gradient `color` does not animate, so lit fills snap on a theme switch
  where flat ones cross-fade
  ([gradient-color-transition](../backlog/gradient-color-transition.md)).
- Labels sit low in their boxes at some base sizes, 15px worst
  ([text-label-centering](../backlog/text-label-centering.md)).
- Bevels at 1x read as edges on the two displays tested; strengths may still
  want to scale with displayScale elsewhere.
- Partial repaint was not checked against glow drawn through an extra rect
  (no clipped or stale glow was seen).
