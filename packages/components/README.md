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
      <SafeArea top bottom>
        <text>Content clear of system UI</text>
      </SafeArea>
    </window>
  )
}
```

Top and bottom insets are applied by default. Pass `false` to opt out of an edge, or a number to apply the inset with a minimum padding.

```jsx
// top only
<SafeArea bottom={false}>

// apply top and bottom insets, with a minimum of 16px each
<SafeArea top={16} bottom={16}>

// all four edges
<SafeArea top bottom left right>
```

**Props**

| Prop       | Type                | Default | Description                                         |
| ---------- | ------------------- | ------- | --------------------------------------------------- |
| `top`      | `boolean \| number` | `true`  | Apply top inset. A number sets the minimum padding. |
| `bottom`   | `boolean \| number` | `true`  | Apply bottom inset. A number sets the minimum padding. |
| `left`     | `boolean \| number` | `false` | Apply left inset. A number sets the minimum padding. |
| `right`    | `boolean \| number` | `false` | Apply right inset. A number sets the minimum padding. |
| `children` | `any`               | -       | Content to render inside the safe area.             |

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

### Image

Loads and displays an image from a URL or raw bytes.

```jsx
import { Image } from "@solidrt/components"

function Avatar() {
  return <Image src="https://example.com/avatar.png" width={64} height={64} />
}
```

**Props**

Accepts all layout, transform, and pointer event props, plus:

| Prop  | Type                   | Description                                  |
| ----- | ---------------------- | -------------------------------------------- |
| `src` | `string \| Uint8Array` | URL to fetch, or raw image bytes to decode   |

## License

MIT. Copyright (c) 2026 Antoine van Wel.
