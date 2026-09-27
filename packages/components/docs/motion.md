# PressFeedback

The hover/pressed tint every pressable face draws over its own fill: one always-mounted detached rect in `overlayHover` while hovered and `overlayPressed` while pressed, at alpha 0 otherwise, fading at the theme's feedback speed. Surface draws it as its tint layer; it is exported for a control that draws its own rows over a face it does not own (the option rows of a Select or ContextMenu).

```jsx
import { PressFeedback, createPress } from "@solidrt/components"

let press = createPress(props)
<view ref={press.ref} {...press.handlers} padding={8}>
  <PressFeedback pressed={press.pressed()} hovered={press.hovered()} radius={6} />
  <text>{props.label}</text>
</view>
```
