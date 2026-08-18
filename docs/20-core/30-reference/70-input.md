# Input

Pointer, wheel, key and text events are props on any element. There is no
`addEventListener` and no event registry: a handler prop is the subscription.

Events travel from the hit leaf up to the root, and `stopPropagation()` ends
the walk.

## Handlers

{{ decl packages/core/src/types.d.ts PointerProps }}

`pointerEvents="none"` takes an element out of hit testing, and the walk skips
it as an ancestor too, so the `parentX`/`parentY` of an event stay in the frame
you would expect.

## Pointer events

Coordinates are logical points, so a handler reads the same numbers on a
high-density phone screen as on a desktop monitor. Three frames are reported,
which is what makes drag idioms short:

{{ decl packages/core/src/types.d.ts PointerEvent }}

The drag idiom is `x = parentX - grab offset`, taking the grab offset from
`localX`/`localY` at pointer down.

{{ decl packages/core/src/types.d.ts WheelEvent }}

## Key events

Key events use the W3C UI Events vocabulary: `key` is the logical,
layout-dependent value (`"a"`, `"!"`, `"Enter"`, `"ArrowLeft"`), and `code` is
the physical key position (`"KeyA"`, `"Digit1"`).

{{ decl packages/core/src/types.d.ts KeyEvent }}

Routing differs from pointer events in where the walk starts: keydown and
keyup dispatch along the focused node's ancestor chain and always end at the
window root, and with nothing focused they go to the window root alone.
`<window onKeyDown>` is therefore the app-global shortcut point.

## Text entry

Printable characters do not arrive as key events. Text entry goes to the
focused node's `onTextInput`:

{{ decl packages/core/src/types.d.ts TextEvent }}

Focusing a field never raises the on-screen keyboard by itself. A tap on the
field does, or an explicit `startTextInput()`, and never while a physical
keyboard is attached. `textInputHints` on the node is read when a session
starts.
