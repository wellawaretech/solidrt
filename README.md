# SolidRT

A low-level toolkit for creating cross-platform applications.

_SolidRT is in pre-alpha stage. Anything can and will be changed._

## Getting started

```sh
bun init
bun add @solidrt/core
bun add -d @solidrt/cli
```

Create an entry file `src/index.tsx`:

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

Run the app:

```sh
srt run src/index.tsx
```

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for how to build from source.

## License

MIT. Copyright (c) 2026 Antoine van Wel.

