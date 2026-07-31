// parseSvg turns a whole SVG *document string* into plain draw data. This is
// not HTML: there are no <rect>/<circle>/<path> JSX children to nest. You hand
// it the source text (a string you import, fetch, or inline) and get back the
// document's intrinsic size plus a flat list of draws whose keys match the
// path element's props - so rendering is a map to <d-path>, wrapped in a view
// whose `viewBox` fits the document's coordinate space into the box.
//
// The point of draws-as-data over an opaque document element: every shape is
// a real node you own. Below, the house highlights the shape under the
// pointer - exact-geometry hit testing (the path outline, not its box), with
// the recolor a per-node prop override that never re-parses the document.
// The same structure gives per-shape animation (wrap a subset in <d-view>),
// layer filtering, or interleaving your own JSX between document layers.
//
// A multi-color document keeps each shape's own fill; a monochrome icon
// using stroke/fill "currentColor" is recolored by the `color` option,
// exactly as `currentColor` would be in a browser. That is how you use an
// existing icon library (Lucide, Heroicons, Feather, Material, ...): they
// ship SVG source strings following the currentColor convention. Parse once
// per document under a memo (or at module scope for a static asset).
//
// Being vectors, the draws are resolution-independent: crisp at any drawn
// size x displayScale(). Prefer them over a raster <texture> (image.tsx)
// whenever the render size is fluid or the display DPI varies.
import { render, parseSvg, svg, createMemo, createSignal, For } from "@solidrt/core"

// Multi-color document: each shape carries its own fill. The `svg` tag returns
// the string unchanged; it exists so editors highlight the markup (like `glsl`
// for shader sources).
const HOUSE = svg`
<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
  <rect x="20" y="45" width="60" height="45" fill="#457b9d"/>
  <path d="M10 50 L50 15 L90 50 Z" fill="#e63946"/>
  <rect x="42" y="62" width="16" height="28" fill="#f1faee"/>
  <circle cx="50" cy="35" r="6" fill="#ffd166"/>
</svg>`

// Monochrome icon (Lucide arrow-right) drawn with currentColor, recolored below.
const ARROW = svg`
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
     stroke-linecap="round" stroke-linejoin="round">
  <path d="M5 12h14"/>
  <path d="M12 5l7 7-7 7"/>
</svg>`

// The payoff over the old document-element approach: each draw is its own
// node, so the shape under the pointer lights up - hit on the true outline
// (hovering the sky inside the roof triangle's box does nothing), recolor
// without a re-parse.
function InteractiveHouse() {
  let doc = createMemo(() => parseSvg(HOUSE))
  let [hot, setHot] = createSignal(-1)
  // repaintBoundary still pays off on an interactive document: the subtree
  // re-records only when a draw inside it changes (hover), never because a
  // sibling elsewhere on the screen did.
  return (
    <view repaintBoundary width={240} height={240} viewBox={[doc().width, doc().height]}>
      <For each={doc().draws}>
        {(draw, i) => (
          <d-path
            {...draw}
            color={hot() === i() ? "#ffd166" : draw.color}
            onPointerEnter={() => setHot(i())}
            onPointerLeave={() => setHot((v) => (v === i() ? -1 : v))}
          />
        )}
      </For>
    </view>
  )
}

// The plain pattern: memoized parse, viewBox-fitted box, draws mapped to
// <d-path>, and a plain repaintBoundary (the DL-reuse tier, not "snapshot")
// so the static subtree never re-records alongside animating siblings. The
// components-package Icon is this plus theming.
function Svg(props: { src: string; size: number; color?: string }) {
  let doc = createMemo(() => parseSvg(props.src, { color: props.color }))
  return (
    <view repaintBoundary width={props.size} height={props.size} viewBox={[doc().width, doc().height]}>
      <For each={doc().draws}>{(draw) => <d-path {...draw} />}</For>
    </view>
  )
}

function App() {
  return (
    <window justifyContent="center" alignItems="center" flexDirection="row" gap={32}>
      <InteractiveHouse />
      <Svg size={120} src={ARROW} color="#4f8cff" />
    </window>
  )
}

render(() => <App />)
