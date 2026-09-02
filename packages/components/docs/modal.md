# Modal

A centered overlay rendered at the window root via core `createPortal`: it fills the window with a dimming backdrop (theme `scrim`; override via `backdropColor`, `"transparent"` for no dim) and centers `children` on top, the whole overlay fading in at mount and out on removal (an exiting modal takes no hits). Control visibility by mounting/unmounting it, e.g. `<Show when={open()}>`; the gating signal must start false since portals cannot mount during the initial render. Pressing the backdrop calls `onClose` (unless `dismissable` is false), pressing the content does not, and while mounted the modal traps `createFocusNav` inside itself.

```jsx
import { Modal, Card, Button } from "@solidrt/components"

<Show when={open()}>
  <Modal onClose={() => setOpen(false)}>
    <Card>
      <Button onPress={() => setOpen(false)}>Close</Button>
    </Card>
  </Modal>
</Show>
```
