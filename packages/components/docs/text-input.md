# TextInput

Text input, single-line by default; `multiline` wraps at the field's width and edits across lines (Enter inserts a newline, Up/Down move by line; grows with content up to `maxRows` unless `layout.height` fixes the box, and scrolls to the caret). Controlled via `value`/`onInput`, or uncontrolled via `defaultValue`; `onSubmit` fires on Enter (single-line only). Also `placeholder`, `maxLength`, `autoFocus`, `disabled`, `onFocus`/`onBlur`, and `hints` for IME behavior (keyboard type, capitalization, autocorrect - identifier-like fields want `{ capitalize: "none", autocorrect: false }`).

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

`style` overrides the themed colors, border, and radius. `autoFocus` focuses on mount (the on-screen keyboard still waits for a tap).
