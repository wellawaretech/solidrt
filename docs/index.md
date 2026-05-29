# SolidRT

A low-level toolkit for building cross-platform applications with SolidJS.

> SolidRT is in pre-alpha stage. Anything can and will be changed.

Write your UI once in JSX. Preview it live on multiple devices simultaneously, controlled from your development machine. No simulators, no emulators, no platform-specific project setup.

Create a fresh Bun project. Add dependencies
`@solidrt/core` and `@solidrt/cli`.

Create file `index.tsx`:

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

```sh
bunx srt run index.tsx
```

and you're good to go!

---


## Documentation

<!-- - [Guide](guide/getting-started.md) - install, first app, dev workflow, performance model -->
- [CLI](reference/cli.md) - `srt` commands and the REPL
- [Core](reference/core.md) - low-level primitives: elements, render, events, GPU
- [Components](reference/components.md) - higher-level components: View, Image, TextInput
- [Flux](reference/flux.md) - JavaScript runtime: file system, networking, events
<!-- - [Internals](internals/architecture.md) - how SolidRT works under the hood -->