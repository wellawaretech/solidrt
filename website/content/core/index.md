# Core

The stable spine of SolidRT: `@solidrt/core` links SolidJS reactivity to the
native rendertree. Intrinsic elements (`view`, `text`, `image`, ...),
reactive primitives (`createSignal`, `createCamera`, `createMicrophone`,
...), layout, input, and the capabilities model for adapting to the device
you are running on.

If you only learn one layer, learn this one. Everything else - frameworks,
tools - is built on it and replaceable; Core is the part that holds still.

## Planned here

- **Concepts** - the mental model: the rendertree, props as reactive values,
  control flow without a virtual DOM, capabilities and environment.
- **Guides** - task-shaped recipes: animate this, lay out that, react to
  input.
- **Examples** - generated from the repository's `examples/`, so they cannot
  drift from the code.
- **Reference** - the full API, generated from the published types.
