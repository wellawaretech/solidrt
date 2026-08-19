# Portal

Renders its child somewhere other than its lexical position: by default at the window root, so overlays (modals, menus, tooltips) escape the clipping and stacking of their surrounding layout; `mount` targets another node captured from a `ref` instead. A thin JSX wrapper over core `createPortal`. The child should be a single element with `position="absolute"`, since it is inserted into the window's flex root. Portals cannot mount during the app's initial render, so gate them behind a signal that starts false.

```jsx
import { Portal } from "@solidrt/components"

<Show when={open()}>
  <Portal>
    <view position="absolute" right={16} bottom={16}>
      <Card>Saved</Card>
    </view>
  </Portal>
</Show>
```
