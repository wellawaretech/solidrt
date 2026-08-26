# ScrollView

A scrollable region; vertical by default, `horizontal` to flip. Both the wheel and dragging scroll the content: the drag activates after a small movement threshold along the scroll axis, also when it starts on a pressable (the press is cancelled and its feedback retracts), and keeps scrolling when the pointer leaves the box. Scrolling glides: the offset springs to each new target (250 ms, critically damped), so a wheel notch never jumps and a burst of notches reads as one motion; a dragging finger is tracked exactly, without the spring. No momentum/fling yet.

```jsx
import { ScrollView, Text } from "@solidrt/components"
import { For } from "@solidrt/core"

<ScrollView layout={{ height: 300 }} style={{ backgroundColor: "#111", borderRadius: 8 }}>
  <For each={items()}>{(item) => <Text>{item}</Text>}</For>
</ScrollView>
```

A `scrollX`/`scrollY` entry in `transition` replaces the default spring: `transition={{ scrollY: { duration: 400, bounce: 0.2 } }}` (keep it a spring rather than a tween, because the wheel retargets mid-flight). The other entries animate the box itself and its background/border as on any component.

The underlying geometry primitive `createScroll` is available from `@solidrt/core` for building custom scrollers.
