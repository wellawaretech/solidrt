# RichTextEditor

Edits a rich text `Document` (styled runs, paragraph attributes) in the same field as `TextInput`: always multiline, same caret, keys, wrapping, and scrolling. Controlled via `value`/`onInput`, or uncontrolled via `defaultValue` (start from `plainDocument("")`). Formatting is driven through `editorRef`, which hands you the document buffer - the component ships no toolbar; the app renders its own controls.

```jsx
import { RichTextEditor, plainDocument } from "@solidrt/components"

function Notes() {
  let editor
  return (
    <>
      <Button onPress={() => editor.format({ bold: editor.attributes().bold ? null : true })}>B</Button>
      <RichTextEditor
        defaultValue={plainDocument("Start typing...")}
        editorRef={(e) => (editor = e)}
        layout={{ width: 320 }}
        maxRows={10}
      />
    </>
  )
}
```

Drawn attributes - inline: `bold`, `italic`, `underline`, `code` (mono), `color` (a color string), `link` (a URL string: primary color, underlined); block: `heading: 1 | 2 | 3`. Other attributes are carried in the document and ignored by the drawing; font-affecting ones feed the text geometry too, so caret and wrap follow the drawn glyphs. Inline atoms (U+FFFC) render as their placeholder character for now.
