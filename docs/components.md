# Components

SolidRT provides _low-level_ primitives. Its core does not understand "images" - it only knows textures. There are no "input fields" - it can only detect key presses.

This package provides several higher-level components which serve as a starting point for regular applications.

```sh
bun add @solidrt/components
```

## Theming

Appearance (colors, spacing, border radii, font sizes) is controlled via the shared theme. Call `setTheme` to override defaults:

```js
import { setTheme } from "@solidrt/components"

setTheme({
  color: {
    text: "#222",
    surface: "#eee",
  },
})
```

---

## Layout and style

Most components group their props into two objects:

- `layout` - properties that feed the layout engine (flexbox/grid, sizing, padding, margin, position). Changing them triggers a relayout. This is the core `LayoutProps` set.
- `style` - paint-only properties that never affect layout: `color`, `backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, and the transform `x`, `y`, `rotate`, `scale`.

Event handlers (`onPointerDown`, `onKeyDown`, etc.) are passed directly as top-level props.

`StyleProps`:

| Prop              | Type                                         | Description                            |
| ----------------- | -------------------------------------------- | -------------------------------------- |
| `color`           | `string`                                     | Text color (used by `Text`).           |
| `backgroundColor` | `string`                                     | Fill color.                            |
| `borderColor`     | `string`                                     | Stroke color (when `borderWidth` set). |
| `borderWidth`     | `number`                                     | Border stroke width.                   |
| `borderRadius`    | `number \| [number, number, number, number]` | Corner radius.                         |
| `x` / `y`         | `number`                                     | Translation offset.                    |
| `rotate`          | `number`                                     | Rotation.                              |
| `scale`           | `number`                                     | Scale factor.                          |

---

## Window

The root surface of an app. Accepts `layout` and the `backgroundColor` from `style`; a window cannot be transformed or bordered.

```jsx
import { Window } from "@solidrt/components"

function App() {
  return (
    <Window title="My App" style={{ backgroundColor: "#111" }}>
      {/* ... */}
    </Window>
  )
}
```

**Props**

| Prop         | Type          | Default | Description                        |
| ------------ | ------------- | ------- | ---------------------------------- |
| `title`      | `string`      | -       | Window title.                      |
| `fullscreen` | `boolean`     | -       | Open fullscreen.                   |
| `layout`     | `LayoutProps` | -       | Layout properties.                 |
| `style`      | `StyleProps`  | -       | Only `backgroundColor` is applied. |
| `children`   | `any`         | -       | Content.                           |

---

## View

A general-purpose box. Spreads `layout` onto the underlying view, applies the transform from `style`, and draws a background and/or border when those style props are set.

```jsx
import { View } from "@solidrt/components"

;<View
  layout={{ padding: 16, flexDirection: "column", gap: 8 }}
  style={{ backgroundColor: "#222", borderRadius: 8 }}
>
  {/* ... */}
</View>
```

**Props**

Accepts all pointer event props, plus:

| Prop       | Type                             | Description        |
| ---------- | -------------------------------- | ------------------ |
| `layout`   | `LayoutProps`                    | Layout properties. |
| `style`    | `StyleProps`                     | Paint properties.  |
| `ref`      | `(node: { id: number }) => void` | Node reference.    |
| `children` | `any`                            | Content.           |

---

## Text

Renders text inside a layout box. Font properties live in `layout` (they affect measurement); `color` lives in `style`.

```jsx
import { Text } from "@solidrt/components"

;<Text layout={{ fontSize: 18, maxLines: 2 }} style={{ color: "#fff" }}>
  Hello
</Text>
```

`layout` accepts all `LayoutProps` plus the font fields `fontFamily`, `fontSize`, `lineHeight`, `fontStyle`, `fontWeight`, `textAlign`, and `maxLines`.

**Props**

Accepts all pointer event props, plus:

| Prop       | Type                             | Description                         |
| ---------- | -------------------------------- | ----------------------------------- |
| `layout`   | `TextLayoutProps`                | Layout properties plus font fields. |
| `style`    | `StyleProps`                     | `color` and transform.              |
| `ref`      | `(node: { id: number }) => void` | Node reference.                     |
| `children` | `any`                            | Text content.                       |

---

## Image

Loads and displays an image from a URL or raw bytes. Handles fetching, decoding, and GPU upload automatically.

```jsx
import { Image } from "@solidrt/components"

function Avatar() {
  return <Image src="https://example.com/avatar.png" layout={{ width: 64, height: 64 }} />
}
```

Accepts raw bytes directly, for cases where the image data is already available:

```jsx
<Image src={imageBytes} layout={{ width: 64, height: 64 }} />
```

**Props**

Accepts `layout`, `style`, and all pointer event props, plus:

| Prop  | Type                   | Description                                |
| ----- | ---------------------- | ------------------------------------------ |
| `src` | `string \| Uint8Array` | URL to fetch, or raw image bytes to decode |

---

## TextInput

Single-line text input. Handles focus, keyboard activation, and text editing.

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
      layout={{ width: 240 }}
    />
  )
}
```

**Props**

| Prop           | Type                      | Default | Description                                                  |
| -------------- | ------------------------- | ------- | ------------------------------------------------------------ |
| `value`        | `string`                  | -       | Controlled value. If omitted, the component is uncontrolled. |
| `defaultValue` | `string`                  | `""`    | Initial value for uncontrolled use.                          |
| `onInput`      | `(value: string) => void` | -       | Fires on every change.                                       |
| `onSubmit`     | `(value: string) => void` | -       | Fires on Enter.                                              |
| `onFocus`      | `() => void`              | -       | Fires when the field gains focus.                            |
| `onBlur`       | `() => void`              | -       | Fires when the field loses focus.                            |
| `placeholder`  | `string`                  | -       | Shown when value is empty and the field is not focused.      |
| `maxLength`    | `number`                  | -       | Truncates input to this length.                              |
| `disabled`     | `boolean`                 | `false` | Ignores pointer and key events when true.                    |
| `autoFocus`    | `boolean`                 | `false` | Focuses on mount.                                            |
| `layout`       | `LayoutProps`             | -       | Layout properties (e.g. `width`).                            |
| `style`        | `StyleProps`              | -       | Overrides theme colors, border, and radius.                  |

---

## SafeArea

Wraps its children in a view that applies padding to avoid system UI intrusions (status bars, home indicators, notches, etc.).

```jsx
import { SafeArea } from "@solidrt/components"

function App() {
  return (
    <Window>
      <SafeArea>
        <Text>Content clear of system UI</Text>
      </SafeArea>
    </Window>
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

| Prop       | Type                | Default | Description                                            |
| ---------- | ------------------- | ------- | ------------------------------------------------------ |
| `top`      | `boolean \| number` | `true`  | Apply top inset. A number sets the minimum padding.    |
| `bottom`   | `boolean \| number` | `true`  | Apply bottom inset. A number sets the minimum padding. |
| `left`     | `boolean \| number` | `false` | Apply left inset. A number sets the minimum padding.   |
| `right`    | `boolean \| number` | `false` | Apply right inset. A number sets the minimum padding.  |
| `relative` | `boolean`           | `true`  | Sets `position="relative"`.                            |
