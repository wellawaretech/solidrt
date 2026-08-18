# Extensions

Extensions are built on top of [Core](/core/): component sets, theming, and
app structure. They are siblings, not a stack. You pick one, some, or none,
and you can always drop down to Core underneath, because an extension
component is just a component that returns Core elements.

Today there are two, both written by the SolidRT project: [Components](/extensions/components/)
and [3D](/extensions/3d/). There is room for others, including community and
commercial ones.

## Components

*Status: evolving.*

`@solidrt/components` is the official component extension: the widgets you
would otherwise write yourself for every app.

- **Input:** `Button`, `TextInput`, `Checkbox`, `RadioGroup` with `Radio`, `Switch`,
  `Slider`, `Select`, `SegmentedControl`, `Pressable`.
- **Structure:** `Window`, `View`, `Card`, `Divider`, `ScrollView`,
  `SplitView`, `NavShell`, `SafeArea`, `Portal`.
- **Content and feedback:** `Text`, `Image`, `Icon`, `Badge`, `Spinner`,
  `ProgressBar`, `Modal`, `Tooltip`, `ContextMenu`, `QrCode`.

Start a project on it by picking `@solidrt/components` in the scaffolder,
or naming it outright:

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

Every gesture in the app goes through Core's single gesture arena, so a
press inside a scrolling list resolves the way you expect: the press claims
provisionally, and a pan that passes its slop steals the gesture or lets it
go. Cross-cutting behavior (motion, density, focus ring) lives in a policy
layer, derived from Core's `capabilities` and `env`, rather than in each
widget.

### What "evolving" means

The APIs still change between releases. Build with it, and expect to move
with it. Core underneath is the layer that changes least, which is why the
[Start](/start/) walkthrough teaches Core first.

The full component list with props and examples is under
[@solidrt/components](/extensions/components/).

## 3D

*Status: experimental.*

`@solidrt/3d` is a retained 3D scene graph above Core's GPU layer: meshes,
materials, and a camera declared as Solid components, rendered into an
ordinary texture in your UI tree. A static scene costs zero GPU passes.

```tsx
import { box, Mesh, PerspectiveCamera, Scene, unlit } from "@solidrt/3d"

<window>
  <Scene width={720} height={720}>
    <PerspectiveCamera position={[0, 1.5, 3]} lookAt={[0, 0, 0]} />
    <Mesh geometry={box()} material={unlit({ color: [0.9, 0.3, 0.3] })} />
  </Scene>
</window>
```

Add it with `--with @solidrt/3d` in the scaffolder. Expect more API churn
here than in the rest of SolidRT. Overview and the export surface are under
[@solidrt/3d](/extensions/3d/).
