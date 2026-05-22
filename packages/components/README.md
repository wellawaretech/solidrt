# @solidrt/components

A collection of components for [SolidRT](https://github.com/wellawaretech/solidrt) apps.

## Installation

```sh
bun add @solidrt/components
```

## Components

### SafeArea

Wraps its children in a view that applies padding to avoid system UI intrusions (status bars, home indicators, notches, etc.).

```jsx
import { SafeArea } from "@solidrt/components"

function App() {
  return (
    <window flexDirection="column">
      <SafeArea>
        <text>Content clear of system UI</text>
      </SafeArea>
    </window>
  )
}
```

**Props**

| Prop       | Type                                         | Default             | Description                                                    |
| ---------- | -------------------------------------------- | ------------------- | -------------------------------------------------------------- |
| `edges`    | `("top" \| "bottom" \| "left" \| "right")[]` | `["top", "bottom"]` | Which edges to apply safe area insets on                       |
| `minimum`  | `number`                                     | `0`                 | Minimum padding applied even if the safe area inset is smaller |
| `children` | `any`                                        | -                   | Content to render inside the safe area                         |

## License

MIT. Copyright (c) 2026 Antoine van Wel.
