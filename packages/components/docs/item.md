# Item

A list row: `startContent` (icon, avatar, checkbox), a `label` with an optional `description` under it, and `endContent` (badge, timestamp, action) pushed to the end. String/number label and description render as themed body and muted body text; anything else as-is. The dense-data workhorse: rows compose with `<For>` inside a plain column view or `ScrollView` - there is no List wrapper, because a column IS the list. Paddings and gaps are density-scaled, so a `<Density>` region compacts rows wholesale.

```jsx
import { Item, Badge, Icon } from "@solidrt/components"
import { For } from "@solidrt/core"

<view flexDirection="column">
  <For each={issues()}>
    {(issue) => (
      <Item
        startContent={<Icon src={Bug} />}
        label={issue.title}
        description={issue.assignee}
        endContent={<Badge variant="neutral">{issue.id}</Badge>}
        selected={issue.id === current()}
        onPress={() => setCurrent(issue.id)}
      />
    )}
  </For>
</view>
```

With `onPress` the row is interactive: hover/pressed overlay tints (no scale - rows sit flush in a list), focusable for spatial navigation, Enter/remote activation, and a focus ring under the `focusRing` policy. An async `onPress` (returning a promise) is not re-fired until it settles. Without `onPress` the row attaches no press recognizer, so controls inside it (a Switch in a settings row) and enclosing pressables receive pointer events untouched; interactivity is decided at mount. `selected` fills the row with `surfaceAlt`; `disabled` dims the row and takes no pointer events. Separate rows with `Divider` where needed.
