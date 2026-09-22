# Dismissible

Swipe-to-dismiss. The content follows the finger sideways; a swipe that qualifies (enough travel and speed, within 30 degrees of the axis, core's swipe recognizer) carries it out of the box at its own speed and reports `onDismiss` with the direction, a drag that stops short springs back. `direction` narrows which way dismisses (`"left"`, `"right"`, default both); a drag the other way is not started, and a vertical drag is left to an enclosing ScrollView. The recognizer takes the pointer at its movement slop, so a pressable row still presses on a tap and retracts once the drag is one. Mouse and touch alike.

```jsx
import { Dismissible, Item } from "@solidrt/components"
import { For } from "@solidrt/core"

<For each={mails()}>
  {(mail) => (
    <Dismissible direction="left" onDismiss={() => archive(mail.id)}>
      <Item label={mail.subject} description={mail.from} onPress={() => open(mail)} />
    </Dismissible>
  )}
</For>
```

The box does not remove itself: after `onDismiss` the content sits parked past the edge until the caller drops the row (a `layout` or `exit` transition on the row then plays the collapse).
