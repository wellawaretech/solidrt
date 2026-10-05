# @solidrt/2d

An instanced sprite layer for SolidRT: your atlases bound together, one GPU
buffer, thousands of sprites in a single draw call, composited into your
app as an ordinary texture element.

*Status: experimental. Expect API churn.*

```tsx
import { decodeImage, render } from "@solidrt/core"
import { createAtlas, grid, Sprite, SpriteLayer } from "@solidrt/2d"
import sheet from "./sheet.png" with { type: "binary" }

let atlas = createAtlas(decodeImage(sheet), { filter: "nearest" })
let frames = grid(atlas, 4, 4)

render(() => (
  <window>
    <SpriteLayer width={720} height={480} atlases={[atlas]}>
      <Sprite x={100} y={120} w={32} h={32} frame={frames[0]} />
      <Sprite x={200} y={160} w={32} h={32} frame={frames[5]} rotation={0.4} />
    </SpriteLayer>
  </window>
))
```

A frame knows the sheet it is cut from, so a layer declaring several
atlases draws sprites from any of them in the same draw (the multi-texture
batch), and a mipmapped sheet goes through `extrudeGrid`/`extrudeRects`
first so its mip chain does not bleed across cells.

The rendertree already handles 2D well: a `d-texture` with atlas sub-rects is
a sprite, with native transitions, gesture-arena pointer events, and
per-element inspectability, and it carries populations into the low
thousands. This package is for what lies beyond that: dense, per-frame
animated populations - entities, particles, bullets - where per-sprite
property writes through the JS boundary are the bottleneck. Sprite records
live in one `Float32Array` and publish to the GPU through SolidRT's zero-copy
buffer write lease; moving ten thousand sprites is ten thousand float stores
and one bulk publish, not twenty thousand FFI calls.

Two layers, like `@solidrt/3d`: an imperative core
(`createSpriteLayer` / `addSprite` / `setSprite` - per-frame motion calls
these from `onFrame`) and the component face (`SpriteLayer` / `Sprite`) for
structure and slow state. A static layer publishes nothing and costs nothing;
the paint-order story is the painter's algorithm (insertion order); pointer
events hit-test exact rotated rects, topmost first, with pointer capture.

In the package: atlas creation, slicing and extrusion (`createAtlas`,
`grid`, `namedFrames`, `fullFrame`, `extrudeGrid`, `extrudeRects`), the
sprite layer with its views and cameras, sprite pointer events, the baked
tile layer, frame animation, key ordering, and the component face. Staged
next: streaming tile worlds and the retro presets (pixel canvas, palette
and scanline passes) - see `okf/backlog/`.

The full model and the sharp edges are in [AGENTS.md](AGENTS.md); runnable
patterns in [examples/](examples/).
