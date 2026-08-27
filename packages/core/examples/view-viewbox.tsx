// `viewBox` on a <view> is the fixed-aspect answer to "many screen sizes": you
// author the whole scene once, in your own made-up design units, and the view
// scales that space to fit its box. It is SVG's viewBox generalized to any
// subtree, not an SVG-only thing.
//
// Four facts, each demonstrated below:
// 1. The view sizes like a REPLACED element (think <img>): its intrinsic size
//    is the design size, one sized axis derives the other from the design
//    aspect, and layout props override it as usual - here `flex={1}` takes
//    the whole window. It never refuses to shrink: a design has no size it
//    cannot scale below.
// 2. It fits UNIFORMLY and centers - one scale for both axes, SVG's default
//    preserveAspectRatio. Content never stretches; the leftover on the loose
//    axis is letterbox, showing the window background through it.
// 3. Children live in DESIGN space, laid-out ones included. x/y, w/h,
//    fontSize, stroke widths, flex, percentages, text wrapping - everything
//    resolves against the design size, not the box. The box a child inherits
//    IS the design size, so a bare `d-rect` fills the design space, detached
//    text wraps at its width, and a flex row is laid out at the design width
//    whatever the window - nothing reflows on resize, the fit does the work.
// 4. Pointer coordinates arrive in design space too. localX/localY on the
//    viewBox view (and on anything under it) read in design units, so no
//    scale factor is threaded through the app's hit math.
//
// The payoff: no `windowSizeClass` branching, no per-breakpoint sizes, no
// scale factor anywhere. The same code runs unchanged from a desktop window to
// a phone. Reach for reflow (responsive-grid.tsx) only when the layout
// genuinely rearranges across form factors; for content with fixed internal
// geometry - diagrams, slides, dashboards, game boards, emulator screens - fit
// one design space instead.
//
// Resize the window and watch: the scene keeps its aspect ratio, everything
// including text scales together, and the readout reports the same design
// coordinates for the same spot on the scene at any window size.
import { render, createSignal, Show } from "@solidrt/core"

// The design space. Every number below is in these units - invent whatever
// suits the content and stay in it. 640x400 is 16:10.
const DESIGN_W = 640
const DESIGN_H = 400

function App() {
  let [at, setAt] = createSignal<{ x: number; y: number } | null>(null)
  let round = (v: number) => Math.round(v)

  return (
    // The window background is what shows through the letterbox bars.
    <window>
      <d-rect color="#0b0f17" />

      {/* flex={1} overrides the intrinsic design size: the box is the whole
          window, and the fit maps the design into it (fact 1). */}
      <view
        flex={1}
        viewBox={[DESIGN_W, DESIGN_H]}
        onPointerMove={(e) => setAt({ x: e.localX, y: e.localY })}
        onPointerLeave={() => setAt(null)}
      >
        {/* No w/h, so it fills the box it inherits - which under a viewBox is
            the design space (fact 3). Its edges are the letterbox edges. */}
        <d-rect color="#151b28" />

        {/* A scene at literal design coordinates. No breakpoints, no
            windowSize() reads, no scale factor: authored once, at this size. */}
        <d-rect x={40} y={40} w={240} h={140} radius={12} color="#1f6feb" />
        <d-rect x={300} y={40} w={300} h={140} radius={12} color="#3fb950" />
        <d-oval x={40} y={220} w={140} h={140} color="#a371f7" />
        <d-line x1={200} y1={300} x2={600} y2={300} color="#e3b341" strokeWidth={6} />
        <d-text x={64} y={95} fontSize={28} color="#e6e6e6">
          fixed design space
        </d-text>
        <d-text x={324} y={95} fontSize={28} color="#0b0f17">
          {DESIGN_W} x {DESIGN_H}
        </d-text>

        {/* Pointer position in design units (fact 4), drawn in design units.
            Non-keyed Show keeps the marker node mounted across moves (a
            ternary on at() would recreate it every event); the accessor
            reads update its props fine-grained. */}
        <d-text x={200} y={330} fontSize={22} color="#8b949e">
          {at() ? `design x ${round(at()!.x)}, y ${round(at()!.y)}` : "move the pointer over the scene"}
        </d-text>
        <Show when={at()}>
          {(a) => <d-oval x={a().x - 8} y={a().y - 8} w={16} h={16} color="#f85149" />}
        </Show>

        {/* A laid-out row under the viewBox (fact 3): positioned and sized in
            design units, the bar flexing against the design width. Resize the
            window: it scales with the scene instead of reflowing. */}
        <view position="absolute" left={40} right={40} top={364} flexDirection="row" alignItems="center" gap={12}>
          <text fontSize={16} color="#8b949e">
            flex row, laid out in design units
          </text>
          <view flex={1} height={10}>
            <d-rect radius={5} color="#1f6feb" />
          </view>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
