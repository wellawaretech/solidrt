# Policies

Theme answers "how does it look"; policies answer "how does it behave". `policy` is a second reactive layer derived from the platform facts in `@solidrt/core` (`capabilities`, `env`), so components adapt to touch vs. desktop, window size, and display without every app wiring that logic itself. Reads are reactive like `theme`: a window resize or the first mouse move on a touch-capable device updates every consuming component live.

The fields:

- `interaction` (`"touch" | "desktop" | "hybrid"`) - which affordances a component shows (hover states vs. long-press). `Tooltip`, `Select`, and `ContextMenu` fork on it.
- `density` (`"comfortable" | "compact" | "dense"`) - control/hit-target/spacing scale; drives `densityScale()` (1 / 0.85 / 0.7). A `<Density>` region overrides it per subtree.
- `motion` (`"normal" | "reduced" | "none"`) - animation intensity.
- `focusRing` (`boolean`) - whether focused controls draw a visible focus indicator (true when a keyboard or gamepad/remote is present).
- `textScale` (`number`) - multiplier on type-scale font sizes; defaults to the OS text-scale preference.
- `textWeightDelta` (`number`) - weight compensation (steps of 100) for light-on-dark text on low-DPI displays.
- `navigation` (`"bottomTabs" | "rail" | "sidebar"`) - recommended nav layout, derived from the window size class. `NavShell` follows it.
- `layout` (`"singlePane" | "twoPane"`) - recommended pane count, derived from the window size class. `SplitView` follows it.

```jsx
setPolicy({ density: "compact" })    // pin a field, overriding the derived value
setPolicy({ density: undefined })    // hand it back to the resolver
```

`setPolicyResolver((caps) => Policies)` replaces the whole system-derivation function for full custom control; `defaultPolicyResolver` is exported to wrap or extend instead of replacing it outright.
