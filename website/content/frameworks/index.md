# Frameworks

Frameworks are built on top of [Core](/core/): component sets, theming, and
app structure. They are siblings, not a stack. You pick one, or none, and
you can always drop down to Core underneath, because a framework component
is just a component that returns Core elements.

Today there is one, written by the SolidRT project. There is room for
others, including community and commercial ones.

## Components

*Maturity: evolving.*

`@solidrt/components` is the official component framework: the widgets you
would otherwise write yourself for every app.

- **Input:** `Button`, `TextInput`, `Checkbox`, `Radio`, `Switch`,
  `Slider`, `Select`, `SegmentedControl`, `Pressable`.
- **Structure:** `Window`, `View`, `Card`, `Divider`, `ScrollView`,
  `SplitView`, `NavShell`, `SafeArea`, `Portal`.
- **Content and feedback:** `Text`, `Image`, `Icon`, `Badge`, `Spinner`,
  `ProgressBar`, `Modal`, `Tooltip`, `ContextMenu`, `QrCode`.

Start a project on it by ticking `@solidrt/components` in the scaffolder:

```sh
bun create solidrt my-app --with @solidrt/components
```

### Theming

Colors, spacing, and the type scale come from one reactive theme. Switching
it recolors the running UI without remounting anything:

```tsx
import { setTheme, lightTheme } from "@solidrt/components"

setTheme(lightTheme)
setTheme({ color: { primary: "#f04e23" } })  // or override one value
```

### Gestures and policy

Components share a gesture arena, so a press inside a scrolling list
resolves the way you expect: the press stays provisional until the pan
either claims the gesture or lets it go. Cross-cutting behavior (motion,
density, focus) lives in a policy layer rather than in each widget.

### What "evolving" means

The APIs still change between releases. Build with it, and expect to move
with it. Core underneath is the part that holds still, which is why the
[Start](/start/) walkthrough teaches Core first.

A per-framework page with examples and a generated API reference lands here
once type extraction covers components.
