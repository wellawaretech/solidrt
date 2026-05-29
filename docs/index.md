# SolidRT

A low-level toolkit for building cross-platform applications with SolidJS.

> SolidRT is in pre-alpha stage. Anything can and will be changed.

Write your UI once with SolidJS. Preview it live on multiple devices simultaneously, controlled from your development machine. No simulators, no emulators, no platform-specific project setup.

Create a fresh Bun project, add dependencies
`@solidrt/core` and `@solidrt/cli`, create a file `index.tsx`:

```jsx
import { render } from "@solidrt/core"

function App() {
  return (
    <window>
      <text>Hello, World!</text>
    </window>
  )
}

render(() => <App />)
```

Then start your development environment
and you're good to go!

```sh
bunx srt run index.tsx
```


---


## Documentation

<!-- - [Guide](guide/getting-started.md) - install, first app, dev workflow, performance model -->
- [CLI](cli.md) - development environment
- [Core](core.md) - low-level primitives: elements, render, events, GPU
- [Components](components.md) - higher-level components: View, Image, TextInput
- [Flux](flux.md) - JavaScript runtime: file system, networking, events
<!-- - [Internals](internals/architecture.md) - how SolidRT works under the hood -->