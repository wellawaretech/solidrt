# TextInput

Text input, single-line by default; `multiline` wraps at the field's width and edits across lines (Enter inserts a newline, Up/Down move by line; grows with content up to `maxRows` unless its layout sizes the box, and scrolls to the caret). Controlled via `value`/`onInput`, or uncontrolled via `defaultValue`; `onSubmit` fires on Enter (single-line only), `onCancel` on Escape or the platform's back, each followed by the blur. Also `placeholder`, `maxLength`, `autoFocus`, `disabled`, `onFocus`/`onBlur`, and `hints` for IME behavior (keyboard type, capitalization, autocorrect - identifier-like fields want `{ capitalize: "none", autocorrect: false }`). The font fields of `layout` (`fontSize`, `fontFamily`, `lineHeight`, `fontStyle`, `fontWeight`) shape the text as on `Text`, with the rows, caret and scrolling following.

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

`style` overrides the themed colors, border, and radius (`borderWidth: 0` draws no border at all, and no focus ring). The mouse cursor is the I-beam over the field. `autoFocus` focuses on mount (the on-screen keyboard still waits for a tap). A tap anywhere outside the field blurs it.

Leaving the field: Enter submits (`onSubmit` with the value, then the blur); Escape and the platform's back (the Android button, a pad's back button) cancel (`onCancel`, then the blur). `onCancel` only reports: the value stays as it is, and reverting it is the app's call. `onBlur` runs after either, and on a tap outside.

Keys the focused field keeps for itself, which never reach a parent `onKeyDown` or the window's shortcuts: Backspace, Delete, Left and Right, Home and End, Enter (and a remote's center key), Escape, Ctrl/Cmd+A and Ctrl/Cmd+V, Ctrl/Cmd+C and Ctrl/Cmd+X when text is selected, and every printable key while typing. A multiline field also keeps Up and Down. Everything else bubbles on (Tab, Up and Down in a single-line field, PageUp, function keys, Ctrl+S, ...), so a chat field that wants Up for its history wraps the field in a `View` with an `onKeyDown`.

A multiline field sizes like a flex item. Unconstrained, it grows with its content, up to `maxRows` rows and then scrolls. Sized by its layout it is a fixed box that scrolls to the caret: an explicit `height`, or `flexGrow: 1` in a parent with a height (the field fills what is left and scrolls once the text outgrows it), or a parent too small for the content (the field shrinks to fit rather than overflowing).

```jsx
<View layout={{ flexDirection: "column", height: 400 }}>
  <Text>Notes</Text>
  <TextInput multiline layout={{ flexGrow: 1, fontSize: 18 }} />
</View>
```
