// A transcript scroller: a ScrollView with a chat's position policies on top.
// It opens at its newest entry, follows growth while the view is at the end,
// holds when the reader has scrolled back, and rests its entries against the
// bottom while they are shorter than the view. ScrollView stays the generic
// scroller; everything chat-specific lives here, so it can grow (history
// loaded in chunks when scrolling back) without touching it.
import { createEffect, createSignal, getBoundingBox, onLayout, untrack } from "@solidrt/core"
import type { LayoutProps, Scroll } from "@solidrt/core"
import { ScrollView, View } from "@solidrt/components"

export function ChatView(props: { children?: any; layout?: LayoutProps }) {
  let outer: { id: number } | undefined
  let [scroll, setScroll] = createSignal<Scroll>()

  // The scroller's height, measured each layout (its viewport fills it; the
  // scroller carries no padding of its own). The content column takes it as
  // a minimum, so entries shorter than the view sit at its end through
  // justifyContent and, once they overflow, start at the start: the `safe
  // flex-end` the layout engine does not have.
  let [height, setHeight] = createSignal(0)
  onLayout(() => {
    let b = outer && getBoundingBox(outer)
    if (b && b.height !== height()) setHeight(b.height)
  })

  // The first fill (nothing was scrollable before it, whether it mounted with
  // the view or arrived later) lands instantly, as a chat opens at its end.
  // After that, when the range grows and the view was at the previous end
  // (within a pixel), it springs to the new end; a reader who scrolled back
  // is left where they are. The handle sits in a signal because it arrives
  // after this effect's first compute; a plain variable would leave the
  // effect tracking nothing. The offset is read untracked: the policy reacts
  // to the range, not to every scroll.
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

  return (
    <ScrollView ref={(n) => (outer = n)} scrollRef={setScroll} layout={props.layout}>
      <View layout={{ flexDirection: "column", justifyContent: "flex-end", minHeight: height() }}>
        {props.children}
      </View>
    </ScrollView>
  )
}
