# ScrollView

A scrollable region; vertical by default, `horizontal` to flip. Both the wheel and dragging scroll the content: the drag activates after a small movement threshold along the scroll axis, also when it starts on a pressable (the press is cancelled and its feedback retracts), and keeps scrolling when the pointer leaves the box. Scrolling glides: the offset springs to each new target (250 ms, critically damped), so a wheel notch never jumps and a burst of notches reads as one motion; a dragging finger is tracked exactly, without the spring. No momentum/fling yet.

```jsx
import { ScrollView, Text } from "@solidrt/components"
import { For } from "@solidrt/core"

<ScrollView layout={{ height: 300 }} style={{ backgroundColor: "#111", borderRadius: 8 }}>
  <For each={items()}>{(item) => <Text>{item}</Text>}</For>
</ScrollView>
```

`scrollRef` hands out the scroll handle from `createScroll`: `offset()` and `range()` (the largest reachable offset, refreshed each layout) are reactive; `scrollTo({ x, y, behavior })` and `scrollBy({ x, y, behavior })` clamp to the range, an omitted axis stays put, and `behavior: "instant"` writes without the spring (the web's word; `"auto"` and `"smooth"` are the default motion). Scroll policies are written against it in the app. A transcript that opens at its newest message and then follows growth, without yanking a reader who has scrolled back:

```tsx
let [scroll, setScroll] = createSignal<Scroll>()
createEffect(
  () => scroll()?.range(),
  (r, prev) =>
    untrack(() => {
      let s = scroll()
      if (!s || !r) return
      if (!prev || prev.y === 0) s.scrollTo({ y: Infinity, behavior: "instant" })
      else if (s.offset().y >= prev.y - 1) s.scrollTo({ y: Infinity })
    }),
)
<ScrollView scrollRef={setScroll}>...</ScrollView>
```

The range changes whenever the content or the viewport changes size. The first fill (nothing was scrollable before it, whether it mounted with the view or arrived a second later) lands instantly, as a chat opens at its end; after that the view follows the end only if it was at the previous end, and the spring makes that follow a glide. The handle arrives once the component has settled, after an effect's first compute, so hold it in a signal (a setter can be passed as the ref) rather than a plain variable, which the effect would find unset and never track. The offset is read untracked: the policy reacts to the range, not to every scroll.

A `scrollX`/`scrollY` entry in `transition` replaces the default spring: `transition={{ scrollY: { duration: 400, bounce: 0.2 } }}` (keep it a spring rather than a tween, because the wheel retargets mid-flight). The other entries animate the box itself and its background/border as on any component.

The underlying geometry primitive `createScroll` is available from `@solidrt/core` for building custom scrollers.
