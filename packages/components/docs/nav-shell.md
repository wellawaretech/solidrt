# NavShell

An app shell that arranges primary navigation around the content per `policy.navigation`: bottom tabs under it (`bottomTabs`), a narrow rail (`rail`), or a wide sidebar (`sidebar`) beside it. The content is a single stable node; switching arrangement only flips the shell's flex direction and remounts the stateless nav strip, so page state survives a resize across a breakpoint. `items` is a `NavItem[]` (`{ value, label, icon? }`; the icon renders above the label in tabs/rail, beside it in the sidebar); controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Safe areas are the caller's concern: wrap the shell in `SafeArea`.

```jsx
import { NavShell, Icon } from "@solidrt/components"

let items = [
  { value: "home", label: "Home", icon: <Icon src={House} /> },
  { value: "settings", label: "Settings", icon: <Icon src={Cog} /> },
]

<NavShell items={items} value={page()} onChange={setPage} layout={{ flex: 1 }}>
  <Show when={page() === "home"} fallback={<Settings />}>
    <Home />
  </Show>
</NavShell>
```
