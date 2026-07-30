# SolidRT

A low-level toolkit for creating cross-platform applications.

_SolidRT is in alpha: useful today, but APIs are still stabilizing._

> LLM agents: see [AGENTS.md](./AGENTS.md) for a dense, self-contained quickstart.

## Getting started

Prerequisites: [bun](https://bun.sh) (only required for development; not needed to run built apps).

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
bunx srt run src/index.tsx
```

Optionally, create a `tsconfig.json` to enable type recognition for SolidRT elements:

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

## API

See [docs/core.md](https://github.com/wellawaretech/solidrt/blob/main/docs/core.md) for the full API reference.

## License

MIT. Copyright (c) 2026 Antoine van Wel.
