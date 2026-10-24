# Animation

SolidRT has two tools for animation:

- The `transition` prop. You declare the motion on the element, and the
  runtime plays it. No JavaScript runs in each frame.
- `onFrame`. Your code runs before each frame and calculates the motion.

Use `transition` for most motion: a press, a panel that opens, a list that
changes. Use `onFrame` for motion that you calculate from the time, for
example a clock hand or a wave.

## Animate a prop change

Add `transition` to an element. When a listed prop gets a new value, the
element moves to the new value. It does not jump.

```tsx
let [pressed, setPressed] = createSignal(false)

<view
  scale={pressed() ? 0.95 : 1}
  transition={{ scale: { duration: 200, bounce: 0.3 } }}
  onPointerDown={() => setPressed(true)}
  onPointerUp={() => setPressed(false)}
  padding={16}
>
  <d-rect color="#2a3a55" radius={10} />
  <text color="white">Press me</text>
</view>
```

There are two types of motion:

- **A spring.** This is the default. `duration` is the time in ms until
  the motion stops. `bounce` sets the overshoot: 0 (the default) does not
  overshoot, and a positive value overshoots.
- **A tween.** Name a curve to get a tween: `{ duration: 300, curve:
  "ease-out" }`. The curves are `linear`, `ease`, `ease-in`, `ease-out`,
  `ease-in-out`, or four cubic-bezier values.

A string is a short form: `"300ms ease-out"` is a tween, and `"300ms"` is a
spring. `delay` holds the motion before it starts.

The `all` key covers all props of the element that can animate. A string
alone is the short form of `all`:

```tsx
<view opacity={visible() ? 1 : 0} transition="200ms ease-out">
```

The props that can animate are numbers, `color` and `shadow`. For example:
`x`, `y`, `opacity`, `scale`, `rotate`, `radius`, `strokeWidth`, and the
`x`, `y`, `w` and `h` of a detached element. A gradient does not animate.

`onTransitionEnd` tells you when a motion is complete.

## Enter and exit

`from` gives the value at which an element starts when it appears. The
element then moves to the value in its props. `exit` gives the value to
which an element moves when it is removed. The element stays on the screen
until the exit is complete.

```tsx
const SLIDE = { duration: 500, bounce: 0.2, from: -80, exit: 80 } satisfies Transition
const FADE = {
  duration: 350,
  curve: "ease-out",
  from: 0,
  exit: { value: 0, curve: "ease-in", duration: 200 },
} satisfies Transition

<view gap={12} transition={{ stagger: 70 }}>
  <For each={rows()}>
    {(label) => (
      <view x={0} opacity={1} transition={{ x: SLIDE, opacity: FADE }}>
        <text color="white">{label}</text>
      </view>
    )}
  </For>
</view>
```

Each row slides in from the left and fades in. When a row is removed, it
slides to the right and fades out.

The object form of `exit` gives the exit its own motion. An `ease-out`
enter looks wrong when it plays in reverse, so the exit above uses
`ease-in`.

`stagger` on the parent adds a delay to each enter and exit in it: 0 ms for
the first row, 70 ms for the second, 140 ms for the third, and so on.

When an element is removed, the exits of all elements in it also play. A
panel can thus remove all of its rows with one exit each.

## Layout changes

The layout moves an element when the content changes: a row above it is
removed, a list changes its sequence, or a card gets more text. The
`layout` key makes the element slide from its old box to its new box:

```tsx
const SLIDE = { duration: 400, bounce: 0.15 } satisfies Transition

<For each={rows()}>
  {(label) => (
    <view transition={{ layout: SLIDE }} padding={16}>
      <d-rect color="#2a3a55" radius={10} />
      <text color="white">{label}</text>
    </view>
  )}
</For>
```

The slide changes the position and the size. When the size changes, the
children get a new layout in each frame. For example, the text wraps again.

A `layout` slide is relative to the parent. When a parent moves, its
children move with it at once. Put `layout` on each level that must slide.

`layout: true` uses the motion of the `all` key.

## Motion in each frame

`onFrame` calls your function before each frame. It gives the frame time
in ms:

```tsx
let [angle, setAngle] = createSignal(0)
onFrame((time) => setAngle(time / 1000))

<d-view x={100} y={100} rotate={angle()}>
  <d-rect x={-20} y={-20} w={40} h={40} color="tomato" />
</d-view>
```

Use detached elements (`d-*`) for this motion. A detached element has no
layout box, so a new value does not start a layout pass.

The runtime draws a frame only when something changes. While an `onFrame`
function is registered, the app draws each frame. Thus, stop the function
when you do not need it. `onFrame` returns a function that stops it, and it
stops by itself when its component is removed. For a slow change, for
example a countdown, use `onFrame(fn, { demand: false })`: the app draws
only when the function writes a value.

## What to animate

Some props are fast to animate, and some are slow:

- **Fast:** the transform props of a `view` (`x`, `y`, `scale`, `rotate`)
  and `opacity`. The runtime applies them after the layout. The geometry of
  a detached element is also fast.
- **Slow:** the layout props, for example `width`, `height`, `left` and
  `padding`. Each change starts a layout pass for the element and its
  neighbors. Do not write them in each frame. For a change of size, use the
  `layout` slide, or `scale`.

`opacity` on a `view` fades the view and all its children as one group.
This needs an extra layer. To fade one shape, put the alpha in its `color`,
for example `#2a3a5580`. This costs nothing.

## Common errors

**The first value does not animate.** All values before the first frame of
an element are set without motion. To animate the first value, use `from`.

**`stagger` on the animating element does nothing.** Put `stagger` on a
parent. The children must declare `from` or `exit`.

**An enter plays one time.** `from` plays when the element is mounted. To
play it again, mount the element again, for example with a keyed `<Show>`.

**A tween starts again for a new target.** When a tween gets a new target,
it starts again from the current value, with its full duration. A spring
keeps its speed. For motion that can change direction, for example a drag
or a toggle, use a spring.

**A removed element leaves the layout at once.** During its exit, the
element stays on the screen, but its neighbors move into its space
immediately. To make the neighbors slide, give them a `layout` slide.

**`width` and `height` are not in `transition`.** They are layout props.
Use the `layout` slide, `scale`, or the `w` and `h` of a detached element.

## Test an animation

A coding agent that uses [sol mcp](/tools/) can slow the frame clock, stop
it, and move it one frame at a time. It can then take a screenshot of each
step.
