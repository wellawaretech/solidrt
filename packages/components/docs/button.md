# Button

A themed press target over `Pressable`: a padded, centered box with a label. A press shrinks it slightly on a quick spring and tints it with `overlayPressed`; hover tints with `overlayHover` (non-touch policies). `variant` picks the visual role - `primary` (accent fill, the default), `secondary`, `ghost` (no fill until hover), `danger` (destructive) - with fill, tints, and label color from the matching theme tokens; no variant draws a border. `size` (`sm`/`md`/`lg`) pins a minimum width so a row of buttons lines up (a longer label still expands past it); omitted, the button sizes to its content. A string or number child renders as the themed label; any other child renders as-is (an icon, a row, ...).

```jsx
import { Button } from "@solidrt/components"

<Button onPress={save}>Save</Button>
<Button variant="ghost" onPress={cancel}>Cancel</Button>
<Button variant="danger" size="md" onPress={remove}>Delete</Button>
```

Press feedback is a slight scale; hover feedback is the theme's `overlayHover` tint drawn over the fill (non-touch interaction policies only), so it composes with any background, including a caller-set `style.backgroundColor`. `disabled` mutes the colors and takes no pointer events.

An `onPress` returning a promise makes the button an async action: while it is unsettled a centered spinner replaces the label (geometry unchanged, so nothing shifts) and further presses are ignored - a save or submit cannot double-fire.

```jsx
<Button onPress={async () => { await save() }}>Save</Button>
```

A focused Button (see `createFocusNav`) draws a ring in the theme `ring` color under the `focusRing` policy (text-colored by default so it stays visible on primary-filled buttons) and activates on Enter, Space, or a remote's center key. `focusable` (default true) opts out of focus-navigation candidacy; disabled buttons are never candidates.
