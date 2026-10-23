# Reference

Core's API is its types. `@solidrt/core` ships `types.d.ts` and
`jsx-runtime.d.ts` as sources, with a doc comment on every prop that deviates
from the CSS or DOM meaning you already expect, so these pages show those
declarations directly rather than paraphrasing them.

They are grouped by subject, not by element. Most props live on several
elements at once (every element takes pointer handlers, every drawing
primitive takes paint props), so a page per element would print the same
interface a dozen times and still not say where a prop comes from.

- [Elements](/docs/reference/elements/) - `window` and `view`, the structural
  vocabulary
- [Drawing](/docs/reference/drawing/) - `rect`, `oval`, `line`, `path`,
  `texture`, and the paint props they share
- [Text](/docs/reference/text/) - `text` and `span`
- [Detached elements](/docs/reference/detached/) - the `d-*` forms and their
  paint-space geometry
- [Layout](/docs/reference/layout/) - flexbox, the grid subset, and the box
  props
- [Transforms](/docs/reference/transforms/) - post-layout transform, opacity,
  and scroll offsets
- [Input](/docs/reference/input/) - handlers and the event objects they receive
- [Shaders](/docs/reference/shaders/) - the `shader` prop on `window` and
  `view`
- [GPU](/docs/reference/gpu/) - textures, shader targets, pipelines and the
  draw list from `@solidrt/core/gpu`
- [Types](/docs/reference/types/) - the shared aliases

## The element vocabulary

Every JSX intrinsic and the prop interfaces it composes. `ref` is available on
all of them and is left out here.

{{ intrinsics packages/core/jsx-runtime.d.ts }}

Read a row as the sum of its parts: `<rect>` takes paint props, pointer props
and layout props, while `<d-rect>` swaps the layout props for detached
geometry. The pages above cover the interfaces in that table.

## Clearing a prop

Writing `undefined` (or `null`) to a prop resets it to its default - the
value it had before anything was set, per element kind. So the reactive
pattern `scale={style()?.scale}` is safe: when the binding clears, the
element returns to its unset state instead of erroring. On a `span`,
clearing a prop returns the run to inheriting from its paragraph. Content
props are the exception: `text` on a span and `d` on a path require a value.
A wrong-typed value is an error either way.
