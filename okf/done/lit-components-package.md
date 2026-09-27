---
title: Move the lit model into @solidrt/components
description: Measure what the lit look costs on the tablet, the TV and Android, then bring Surface, theme.material and theme.elevation into the package on one shadow rect per surface, migrating every component in one pass with flat as the stock look and lit as a preset.
created: 2026-09-27
completed: 2026-09-27
---

# Move the lit model into @solidrt/components

The model is settled ([lit-components](../design/lit-components.md)); the
prototype in `demo/comp` proves it by eye on a low-DPI Linux display. What
keeps it out of the package is cost on the small targets and the shape of
the migration.

## Done looks like

- The numbers that decide default versus preset (measured, see step 1).
- `Surface` is the one way a component draws its chrome; `theme.material`
  (by role) and `theme.elevation` exist; `theme.components.<name>` gains a
  `material` key so a per-component override can restyle a material without
  a second model. Flat stays the stock look; a lit preset ships beside
  darkTheme and lightTheme.
- Every component migrated in one pass (about twenty draw their own chrome
  today), overlays included: Tooltip, the Select dropdown, ContextMenu and
  the bottom sheets take the overlay role, so glass reaches them.
- The key shadow on the fill rect itself and the contact shadow on one
  opaque rect under it, both under `transition={{ shadow }}`; the
  prototype's cross-faded shadow layers are gone, and no caster is
  transparent except glass.

## Steps

1. **Measure.** Done 2026-09-27: [lit-list-cost](../notes/lit-list-cost.md)
   and [backdrop-filter-cost](../notes/backdrop-filter-cost.md). Lit is a
   preset, not the default; Item is flat by default; Surface casts from
   opaque casters; glass is the overlay role's option.
2. **Surface into the package** on the shadow transition, with the theme
   fields above and the `material` override key. Keep the prototype's
   constants (elevation levels, shadow strength per scheme, the light tint)
   as named constants.
3. **Migrate** all components, the overlays last. Judge by eye on a real
   display, flat and lit side by side, dark and light.
4. **Leftovers from the prototype:** a `<Scene>` backdrop sharing the theme
   light (the `[x, -y, -z]` conversion is derived, not tested); a partial
   repaint check for glow drawn through an extra rect.

Not in scope: a per-frame light (an ideas.md line, needs a shared paint
value in the runtime); controls rendered in 3D (rejected, see the design
document).

## Outcome

Built 2026-09-27, the same day: `Surface` (packages/components/src/surface.tsx)
with `facePaint` for the hand-drawn parts, `light.ts` (the demo's light
model ported, the axis interned as one memo over `theme.light`),
`theme.light`, `theme.shadow`, `theme.material` by role (the glow roles
folded into it as each material's `glow` layer), `theme.elevation`, and
`material`/`elevation` on `StyleProps` and every `theme.components` entry
(`contextMenu` and `navShell` joined the themed set). Every component draws
its faces through Surface: Button, Card (with a `lift` prop), Item, Badge,
Switch, Checkbox, Radio, Slider, ProgressBar, SegmentedControl, Select,
ContextMenu, Tooltip, the editor field behind TextInput and RichTextEditor,
and the NavShell strips and rows. The stock presets are flat and render
pixel-identically to before (the gallery and the theme-toggle example
compared frame to frame; the terminal probe differs only under a 2%
channel tolerance where the glow's caster moved under the fill).
`litDarkTheme` and `litLightTheme` ship beside them, with
packages/components/examples/lit.tsx as the side-by-side page and its
light pad. Shadows cast from opaque casters; Item, Badge and Checkbox sit
flat by default; the press sink runs on the native shadow transition
(verified frame by frame on a desktop client, zero save layers).

Left for the app that prototyped it: its own lit copies can go, the
package's Surface replaces them; its `theme.glow` references become
`theme.material.<role>.glow`. Still open from the design document: a
`<Scene>` backdrop sharing the theme light, and a partial-repaint check for
a glow drawn through an extra rect.
