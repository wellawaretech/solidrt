# Tooltip

A hover-only affordance: under the `desktop`/`hybrid` interaction policies, resting a mouse pointer on the wrapped content shows a bubble near it after `delay` (default 500ms). Under the `touch` policy it never shows, so tooltip content must stay non-essential. The bubble is portal-mounted at the window root, clamped to the window edges, takes no pointer events, fades in and out, and hides on leave and on press. A string/number `content` renders as themed body text; anything else as-is. `placement` picks the side (`"top"`, the default, or `"bottom"`).

```jsx
import { Tooltip, Button } from "@solidrt/components"

<Tooltip content="Save (Ctrl+S)">
  <Button onPress={save}>Save</Button>
</Tooltip>
```
