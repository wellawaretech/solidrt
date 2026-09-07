# createFocusNav

Focus navigation for pointer-free control (TV remote, keyboard, gamepad), moving real focus across the elements declaring `focusable`. It consumes three UI actions from an input map and reads no device: `navigate` (vec2) moves spatially, picking the nearest candidate in the pressed direction by on-screen boxes; `cycle` (axis) walks visual reading order - rows top to bottom, left to right - wrapping at the ends; `select` (button) activates the focused control; a held direction repeats. Created bare, the nav binds the standard set itself (`uiBindings`: arrows, Tab and Shift+Tab, Enter, Space and the remote center key on the keyboard; dpad, left stick and south on every pad) on a map of its own, reachable as `nav.input` for rebinding; given `input`, it consumes the app's map instead. Spread `nav.handlers` on the window. Nothing is focused until the first navigation press; pointer input works unchanged throughout. Every interactive control (Button, Item, TextInput, RichTextEditor, Checkbox, Radio, Switch, Select and its options, SegmentedControl segments, Slider) is a candidate unless disabled, and draws the theme's `ring` color at `borderWidth.focus` while focused under the `focusRing` policy; the Slider steps its value with the arrow keys.

```jsx
import { createFocusNav } from "@solidrt/components"

function App() {
  let nav = createFocusNav()
  return <Window {...nav.handlers}>...</Window>
}
```

Spreading the map's handlers on the window is what keeps it cooperative: key events bubble from the focused node, so a focused TextInput keeps its caret keys (and a typed Space) and navigation only sees what nothing else consumed. Pads need no handler; their sources poll.

Rebind on the nav's own map, or bring a map that also holds the app's actions:

```jsx
let nav = createFocusNav()
nav.input.bind("select", gamepad().button("east"))

let input = createInputMap({ ...uiActions, pause: "button" })
input.bind(uiBindings({ keyboard, gamepad: gamepad() }))
input.bind("pause", keyboard.key("Escape"), gamepad().button("start"))
let nav = createFocusNav({ input })
return <Window {...input.handlers}>...</Window>
```

When the focused control disappears (an action replacing it, a screen change), focus lands on the nearest candidate to where it sat as soon as the successor is laid out - the ring follows a Disconnect button into the Connect button that replaces it. A deliberate blur (tapping outside, dismissing the keyboard) stays blurred; the next press resumes at the nearest candidate.

An open `Modal` traps navigation inside itself with no extra wiring (topmost wins when stacked); pass `scope: () => nodeOrNull` to trap into some other subtree instead. `move`/`tab`/`activate` are exposed for custom triggers.
