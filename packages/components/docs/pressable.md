# Pressable

A pressable box: `onPress` fires on a primary-button press released over the box; a drag out of the box (or a non-primary button) does not fire it, and a drag back in restores the pressed state. `children` and `style` may each be a function of the live `{ pressed, hovered, pending }` state, so the box restyles on press/hover without extra signals - read the state inside the prop or child expression, never eagerly into a local.

```jsx
import { Pressable, Text } from "@solidrt/components"

<Pressable
  onPress={() => setCount((c) => c + 1)}
  layout={{ padding: 12 }}
  style={(s) => ({ backgroundColor: s.pressed ? "#333" : "#222", borderRadius: 8 })}
>
  <Text>Tap me</Text>
</Pressable>
```

`disabled` takes no pointer events. When pressables nest, the innermost one wins the press. An `onPress` returning a promise sets `pending` until it settles; presses meanwhile are ignored, so async actions cannot double-fire.
