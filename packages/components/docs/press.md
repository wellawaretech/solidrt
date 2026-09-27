# createPress

The press recognizer the package's own controls are built on, for a custom control that wants its own root view instead of wrapping `Pressable`: `onPress` fires on a primary-button down followed by an up over the node, a drag out retracts the pressed state and a drag back in restores it, nested recognizers resolve innermost-first through core's arena, and the node's `ref` registers `onPress` as its focus-navigation `select` action so a remote or keyboard activates it while it holds focus. Spread `handlers` on the root view, attach `ref`, and read `pressed()`, `hovered()`, `focused()` and `pending()` (or `state()`, the live object the `Pressable` render prop receives) inside the props that style on them. Options are read at event time, so a component's reactive props object can be passed as is.

```jsx
import { createPress, theme } from "@solidrt/components"

function Chip(props) {
  let press = createPress(props)
  return (
    <view ref={press.ref} {...press.handlers} padding={8} focusable cursor="pointer">
      <d-rect color={press.pressed() ? theme.color.overlayPressed : theme.color.surfaceAlt} radius={theme.radius.full} />
      <text color={theme.color.text}>{props.label}</text>
    </view>
  )
}
```

`registerNavAction` (from `createFocusNav`'s module) is the piece underneath: a control that activates without `createPress` registers its own `select` action with it.
