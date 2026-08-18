# Reference

Core's API is its types. `@solidrt/core` ships `types.d.ts` and
`jsx-runtime.d.ts` as sources, with a doc comment on every prop that deviates
from the CSS or DOM meaning you already expect, so these pages show those
declarations directly rather than paraphrasing them.

They are grouped by subject, not by element. Most props live on several
elements at once (every element takes pointer handlers, every drawing
primitive takes paint props), so a page per element would print the same
interface a dozen times and still not say where a prop comes from.

- [Elements](/core/reference/elements/) - `window` and `view`, the structural
  vocabulary
- [Drawing](/core/reference/drawing/) - `rect`, `oval`, `line`, `path`,
  `texture`, and the paint props they share
- [Text](/core/reference/text/) - `text` and `span`
- [Detached elements](/core/reference/detached/) - the `d-*` forms and their
  paint-space geometry
- [Layout](/core/reference/layout/) - flexbox, the grid subset, and the box
  props
- [Transforms](/core/reference/transforms/) - post-layout transform, opacity,
  and scroll offsets
- [Input](/core/reference/input/) - handlers and the event objects they receive
- [Shaders](/core/reference/shaders/) - the `shader` prop on `window` and
  `view`
- [Types](/core/reference/types/) - the shared aliases

## The element vocabulary

Every JSX intrinsic and the prop interfaces it composes. `ref` is available on
all of them and is left out here.

{{ intrinsics packages/core/jsx-runtime.d.ts }}

Read a row as the sum of its parts: `<rect>` takes paint props, pointer props
and layout props, while `<d-rect>` swaps the layout props for detached
geometry. The pages above cover the interfaces in that table.
