# @solidrt/components

A collection of components for [SolidRT](https://github.com/wellawaretech/solidrt) apps.

## Installation

```sh
bun add @solidrt/components
```

## Theming

Appearance (colors, spacing, border, font size) is controlled via the shared theme. Call `setTheme` from `@solidrt/components` to override defaults.

## Components

### SafeArea

Wraps its children in a view that applies padding to avoid system UI intrusions (status bars, home indicators, notches, etc.).

```jsx
import { SafeArea } from "@solidrt/components"

function App() {
  return (
    <window>
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

### TextInput

Single-line text input.

```jsx
import { TextInput } from "@solidrt/components"
import { createSignal } from "@solidjs/signals"

function NameField() {
  let [name, setName] = createSignal("")
  return (
    <TextInput
      value={name()}
      onInput={setName}
      onSubmit={(v) => console.log("submitted", v)}
      placeholder="Your name"
      width={240}
    />
  )
}
```

**Props**

| Prop           | Type                       | Default | Description                                                  |
| -------------- | -------------------------- | ------- | ------------------------------------------------------------ |
| `value`        | `string`                   | -       | Controlled value. If omitted, the component is uncontrolled. |
| `defaultValue` | `string`                   | `""`    | Initial value for uncontrolled use                           |
| `onInput`      | `(value: string) => void`  | -       | Fires on every change                                        |
| `onSubmit`     | `(value: string) => void`  | -       | Fires on Enter                                               |
| `onFocus`      | `() => void`               | -       | Fires when the field gains focus                             |
| `onBlur`       | `() => void`               | -       | Fires when the field loses focus                             |
| `placeholder`  | `string`                   | -       | Shown when value is empty and the field is not focused       |
| `maxLength`    | `number`                   | -       | Truncates input to this length                               |
| `disabled`     | `boolean`                  | `false` | Ignores pointer and key events when true                     |
| `autoFocus`    | `boolean`                  | `false` | Focuses on mount                                             |
| `width`        | `number \| "auto" \| "N%"` | -       | Field width                                                  |

## License

MIT. Copyright (c) 2026 Antoine van Wel.
