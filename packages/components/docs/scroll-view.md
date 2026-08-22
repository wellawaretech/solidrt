# ScrollView

A scrollable region; vertical by default, `horizontal` to flip. Both the wheel and dragging scroll the content: the drag activates after a small movement threshold along the scroll axis, also when it starts on a pressable (the press is cancelled and its feedback retracts), and keeps scrolling when the pointer leaves the box. No momentum/fling yet.

```jsx
import { ScrollView, Text } from "@solidrt/components"
import { For } from "@solidrt/core"

<ScrollView layout={{ height: 300 }} style={{ backgroundColor: "#111", borderRadius: 8 }}>
  <For each={items()}>{(item) => <Text>{item}</Text>}</For>
</ScrollView>
```

`transition` goes to the viewport, so a scroll offset can spring instead of jump: `transition={{ scrollY: { duration: 250 } }}` makes every wheel notch and `scrollTo` retarget a spring toward the new offset (a spring rather than a tween, because the wheel retargets mid-flight).

The underlying geometry primitive `createScroll` is available from `@solidrt/core` for building custom scrollers.
