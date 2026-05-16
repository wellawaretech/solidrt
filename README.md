# SolidRT

A low-level toolkit for creating cross-platform applications.

_SolidRT is in pre-alpha stage. Anything can and will be changed._

## Getting started

Prerequisites: [bun](https://bun.sh) (only required for development; not needed to run built apps).

```sh
bun init
bun add @solidrt/core
bun add -d @solidrt/cli
```

Optionally, create a `tsconfig.json` to enable JSX support and type recognition for SolidRT elements:

```json
{
  "compilerOptions": {
    "jsx": "preserve",
    "jsxImportSource": "@solidrt/core",
    "moduleResolution": "bundler",
    "strict": true
  }
}
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
bunx srt run src/index.tsx
```

## Development

See [DEVELOPMENT.md](DEVELOPMENT.md) for how to build from source. See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

MIT. Copyright (c) 2026 Antoine van Wel.

