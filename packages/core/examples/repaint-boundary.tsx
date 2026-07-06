// repaintBoundary marks a <view> subtree as its own retained cache, so nearby
// frequently changing content does not force it to rebuild every frame. It has
// two modes - both shown here, next to a square that spins every frame:
//
//  - repaintBoundary={true}  retains the recorded DRAW LIST: the subtree is
//    recorded once and the commands are replayed until something inside changes.
//    Skips re-recording. The default choice for static content next to animation.
//
//  - repaintBoundary="snapshot" additionally retains the RASTERIZED PIXELS as a
//    GPU texture, so replay skips rasterization too. Worth the texture memory
//    only for raster-expensive static content (many glyphs, blurs, dense vector).
//    It re-rasters on layout-size / display-scale changes, crops anything painted
//    outside the layout box, and an ancestor scale animation smears the bitmap -
//    so reach for it only for screen-aligned, static, raster-heavy subtrees.
//
// Rule of thumb: start with {true}; upgrade to "snapshot" only when the cached
// content is expensive to rasterize and stays screen-aligned and static.
import { render, onFrame, createSignal, For } from "@solidrt/core"

// Raster-expensive static content: a dense grid of rects. Cheap to replay from a
// draw list, but re-rasterizing it every frame would be wasteful - the case
// "snapshot" is built for.
function Grid(props: { boundary: true | "snapshot" }) {
  let cells = Array.from({ length: 64 }, (_, i) => i)
  return (
    <view
      repaintBoundary={props.boundary}
      width={200}
      height={200}
      flexDirection="row"
      flexWrap="wrap"
      gap={3}
    >
      <For each={cells}>
        {(i) => <rect width={22} height={22} radius={3} color={i % 2 ? "#2a3f5f" : "#3366b3"} />}
      </For>
    </view>
  )
}

function App() {
  let [angle, setAngle] = createSignal(0)
  onFrame((tick) => setAngle(tick / 1000))

  return (
    <window flexDirection="row" alignItems="center" justifyContent="center" gap={32}>
      <Grid boundary={true} />
      <Grid boundary="snapshot" />
      <view width={120} height={120} rotate={angle()}>
        <rect width={120} height={120} radius={16} color="#e0245e" />
      </view>
    </window>
  )
}

render(() => <App />)
