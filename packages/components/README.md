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

### TextInput

Single-line text input. Tapping/clicking the field focuses it; the OS-level text input (and on-screen keyboard, where applicable) is activated automatically while focused. Printable text arrives via the platform `textInput` event (post-IME commit). V1 supports caret-at-end editing only: no selection, no mid-string cursor movement.

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

| Prop               | Type                       | Default            | Description                                                  |
| ------------------ | -------------------------- | ------------------ | ------------------------------------------------------------ |
| `value`            | `string`                   | -                  | Controlled value. If omitted, the component is uncontrolled. |
| `defaultValue`     | `string`                   | `""`               | Initial value for uncontrolled use                           |
| `onInput`          | `(value: string) => void`  | -                  | Fires on every change                                        |
| `onSubmit`         | `(value: string) => void`  | -                  | Fires on Enter                                               |
| `onFocus`          | `() => void`               | -                  | Fires when the field gains focus                             |
| `onBlur`           | `() => void`               | -                  | Fires when the field loses focus                             |
| `placeholder`      | `string`                   | -                  | Shown when value is empty and the field is not focused       |
| `maxLength`        | `number`                   | -                  | Truncates input to this length                               |
| `disabled`         | `boolean`                  | `false`            | Ignores pointer and key events when true                     |
| `autoFocus`        | `boolean`                  | `false`            | Focuses on mount                                             |
| `fontSize`         | `number`                   | `14`               | Text size                                                    |
| `color`            | `string`                   | `"black"`          | Text color                                                   |
| `placeholderColor` | `string`                   | `"rgba(0,0,0,.4)"` | Placeholder color                                            |
| `background`       | `string`                   | `"white"`          | Background fill color                                        |
| `borderColor`      | `string`                   | `"rgba(0,0,0,.2)"` | Border stroke color                                          |
| `borderWidth`      | `number`                   | `1`                | Border stroke width                                          |
| `borderRadius`     | `number`                   | `4`                | Corner radius                                                |
| `caretColor`       | `string`                   | same as `color`    | Caret color                                                  |
| `padding`          | `number`                   | `8`                | Horizontal padding                                           |
| `width`            | `number \| "auto" \| "N%"` | -                  | Field width                                                  |
| `height`           | `number`                   | `32`               | Field height                                                 |

## License

MIT. Copyright (c) 2026 Antoine van Wel.
