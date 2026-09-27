# Surface

The one way a component draws a face: a role's material (`theme.material`) under the theme light (`theme.light`) at an elevation (`theme.elevation`), stacked as layers that fill the view it is rendered in - a contact shadow, the glow, the fill (tinted by the light, with the sheen gradient along it, carrying the key shadow the elevation casts), the hover/pressed tint, the bevel and the outline. Every themed component draws its chrome through it, so a theme's light, materials and elevation restyle the whole library at once; under the stock presets (flat materials, zero elevation) a face is a plain fill, the tint and the outline, the look the package has always had. A custom control uses it the same way: render it as the first child of the laid-out view it dresses, pick a `role` (never a material), give it the base `fill`, the `radius`, the press state for the tint and the sink, and an `outline` for its border or focus ring.

```jsx
import { Surface, createPress, theme } from "@solidrt/components"

function Tile(props) {
  let press = createPress(props)
  return (
    <view ref={press.ref} {...press.handlers} padding={16} cursor="pointer" focusable>
      <Surface role="surface" elevation="raised" radius={theme.radius.lg} pressed={press.pressed()} hovered={press.hovered()} />
      <text color={theme.color.text}>{props.label}</text>
    </view>
  )
}
```

A pressed face sinks (its key shadow fades toward the contact shadow, its lit edge moves to the far side) on the native shadow transition; a `sunken` face is a well or a groove (shaded like a pressed face, casting nothing); `lifted` with `liftTo` retargets the key shadow to a higher level (the Card hover lift); `chrome={false}` keeps only the tint (a ghost button at rest). `material` merges layers over the role's, `glow` overrides that layer alone. Shadows cast from opaque casters hidden under the fill, since a transparent caster's blurred shadow composites in a save layer of its own, which a tiled GPU pays for per shadow; only a glass material (a frosted backdrop, the overlay role's option) casts from a transparent one. `facePaint` gives the same derivation as plain paint values, for a part a control draws itself (a slider fill, a segmented indicator); `roleFill`, `roleMaterial` and `surfaceSinks` are the pieces it is built from.
