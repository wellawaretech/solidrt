# Element vocabulary

Every JSX intrinsic and the prop interfaces it composes. `ref` is available on
all of them and is left out here.

{{ intrinsics packages/core/jsx-runtime.d.ts }}

Read a row as the sum of its parts: `<rect>` takes paint props, pointer props
and layout props, while `<d-rect>` swaps the layout props for detached
geometry. The [reference pages](/core/#reference) cover the interfaces in
that table.

## Clearing a prop

Writing `undefined` (or `null`) to a prop resets it to its default - the
value it had before anything was set, per element kind. So the reactive
pattern `scale={style()?.scale}` is safe: when the binding clears, the
element returns to its unset state instead of erroring. On a `span`,
clearing a prop returns the run to inheriting from its paragraph. Content
props are the exception: `text` on a span and `d` on a path require a value.
A wrong-typed value is an error either way.
