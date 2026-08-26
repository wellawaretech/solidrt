# SplitView

A list-detail container driven by `policy.layout`: `twoPane` shows the `list` pane (width `listWidth`, default `theme.size.splitViewList`) beside the `detail` pane, `singlePane` shows one at a time per `showDetail`. Keep pane state (selection, scroll) in the app, not in the panes: crossing a breakpoint re-arranges and can remount them. It draws no chrome and adds no padding; a back affordance in the single-pane detail is the app's to render (fork on `policy.layout`).

```jsx
import { SplitView } from "@solidrt/components"

<SplitView
  layout={{ flex: 1 }}
  list={<Inbox onOpen={setSelected} />}
  detail={<Message id={selected()} onBack={() => setSelected(null)} />}
  showDetail={selected() !== null}
/>
```
