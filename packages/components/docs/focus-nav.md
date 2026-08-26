# createFocusNav

Focus navigation for pointer-free control (TV remote, keyboard, gamepad), moving real focus across the elements declaring `focusable`. Two movement types over the same candidates: spatial (arrow keys, dpad) picks the nearest candidate in the pressed direction by on-screen boxes, and sequential (Tab / Shift+Tab) walks visual reading order - rows top to bottom, left to right - wrapping at the ends. Enter / remote center / gamepad south activates the focused control. Nothing is focused until the first navigation press; pointer input works unchanged throughout. Every interactive control (Button, Item, TextInput, RichTextEditor, Checkbox, Radio, Switch, Select and its options, SegmentedControl segments, Slider) is a candidate unless disabled, and draws the theme's `ring` color at `borderWidth.focus` while focused under the `focusRing` policy; the Slider steps its value with the arrow keys.

```jsx
import { createFocusNav } from "@solidrt/components"

function App() {
  let nav = createFocusNav()
  return <window onKeyDown={nav.onKeyDown}>...</window>
}
```

Attaching `nav.onKeyDown` on the window is what keeps it cooperative: key events bubble from the focused node, so a focused TextInput keeps its arrow keys and navigation only sees what nothing else consumed. Gamepad dpad/south are wired automatically.

When the focused control disappears (an action replacing it, a screen change), focus lands on the nearest candidate to where it sat as soon as the successor is laid out - the ring follows a Disconnect button into the Connect button that replaces it. A deliberate blur (tapping outside, dismissing the keyboard) stays blurred; the next press resumes at the nearest candidate.

An open `Modal` traps navigation inside itself with no extra wiring (topmost wins when stacked); pass `scope: () => nodeOrNull` to trap into some other subtree instead. `move`/`tab`/`activate` are exposed for custom triggers.
