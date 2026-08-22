# Types

The shared aliases the element props refer to.

## Color

{{ decl packages/core/src/types.d.ts Color }}

Anywhere a `color` prop is accepted, a `Gradient` from
`createLinearGradient` or `createRadialGradient` is accepted too. Gradient
stops are positions in 0..1, not percentages.

## Percentages

{{ decl packages/core/src/types.d.ts Pct }}

`pct(50)` is the only way to write a percentage as a value. It is branded, so
a percentage cannot be confused with a pixel count by accident, and it
resolves against the element box wherever it is used - layout dimensions,
gaps, and transform origins alike.

## Children

{{ decl packages/core/src/types.d.ts Children }}

The element type is SolidJS's own, so the control-flow components (`For`,
`Show`, `Switch`) return something the JSX types accept.

## JSX plumbing

One declaration exists purely to tell TypeScript which prop receives JSX
children. It is not something an app refers to:

{{ decl packages/core/src/types.d.ts ElementChildrenAttribute }}
