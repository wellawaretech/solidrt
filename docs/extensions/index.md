# Extensions

An extension is a layer above [Core](/core/). Extensions supply component
sets, theming, and app structure. Extensions are parallel. They do not go on
top of each other. You can use one, some, or no extensions. You can always
use Core directly below an extension. An extension component is only a
component that returns Core elements.

At this time, there are three extensions. The SolidRT project wrote all
three: [Components](/extensions/components/), [2D](/extensions/2d/) and
[3D](/extensions/3d/). Other extensions are possible, from the community and
from companies.

## Components

*Status: evolving.*

`@solidrt/components` is the official component extension. It contains the
widgets that you must otherwise write yourself for each app.

- **Input:** `Button`, `TextInput`, `RichTextEditor`, `Checkbox`, `RadioGroup`
  with `Radio`, `Switch`, `Slider`, `Select`, `SegmentedControl`, `Pressable`.
- **Structure:** `Window`, `View`, `Card`, `Divider`, `ScrollView`,
  `SplitView`, `NavShell`, `SafeArea`, `Portal`.
- **Content and feedback:** `Text`, `Image`, `Icon`, `Badge`, `Spinner`,
  `ProgressBar`, `Modal`, `Tooltip`, `ContextMenu`, `QrCode` (in a different
  entry, `@solidrt/components/qrcode`).

To make a project with it, select `@solidrt/components` in the scaffolder.
To add it to a project that you have, use this command:

```sh
bun add @solidrt/components
```

### Theming

Colors, spacing, and the type scale come from one reactive theme. When you
change the theme, the UI that runs gets the new colors immediately. No
component mounts again:

```tsx
import { setTheme, lightTheme } from "@solidrt/components"

setTheme(lightTheme)
setTheme({ color: { primary: "#f04e23" } })  // or override one value
```

### Gestures and policy

All gestures in the app go through the single gesture arena of Core. Thus,
a press inside a list that scrolls operates as you expect. The press claims
the gesture first, but not permanently. A pan that moves past its slop
takes the gesture. Otherwise, the pan releases it.

Behavior that applies to all widgets (motion, density, focus ring) is in a
policy layer, not in each widget. This layer gets its values from the
`capabilities` and `env` of Core.

### What "evolving" means

The APIs can change between releases. You can use the extension in your
app, but expect changes to your code when the APIs change. Core, below the
extension, changes the least. For this reason,
[Basics](/guides/basics/) teaches Core first.

For the full list of components, with props and examples, see
[@solidrt/components](/extensions/components/).

## 2D

*Status: experimental.*

`@solidrt/2d` is the extension for apps and games with much 2D content. Core
already draws 2D content well. A `d-texture` with an atlas sub-rect is a
sprite, with native transitions and pointer events. Core can show a few
thousand of these sprites.

`@solidrt/2d` is for larger quantities. Its instanced sprite layer puts
thousands of sprites into one GPU buffer and one draw call. These sprites
(entities, particles, bullets) can move in each frame. The layer composites
into your UI tree as an ordinary texture. To move ten thousand sprites, the
layer does ten thousand float stores and one publish. It does not do twenty
thousand property writes.

```tsx
import { decodeImage } from "@solidrt/core"
import { createAtlas, grid, Sprite, SpriteLayer } from "@solidrt/2d"
import sheet from "./sheet.png" with { type: "binary" }

let atlas = createAtlas(decodeImage(sheet), { filter: "nearest" })
let frames = grid(atlas, 4, 4)

<window>
  <SpriteLayer width={720} height={480} atlases={[atlas]}>
    <Sprite x={100} y={120} w={32} h={32} frame={frames[0]} />
    <Sprite x={200} y={160} w={32} h={32} frame={frames[5]} rotation={0.4} />
  </SpriteLayer>
</window>
```

A layer declares the atlases that it uses and binds them as one draw. Thus,
sprites from different sheets can mix in any order. Each frame keeps a
reference to its sheet.

To use it, select it in the scaffolder, or use `bun add @solidrt/2d`. For an
overview and the export surface, see [@solidrt/2d](/extensions/2d/).

## 3D

*Status: experimental.*

`@solidrt/3d` is a retained 3D scene graph above the GPU layer of Core. You
declare meshes, materials, and a camera as Solid components. The scene
renders into an ordinary texture in your UI tree. A static scene uses no GPU
passes.

```tsx
import { box, Mesh, PerspectiveCamera, Scene, unlit } from "@solidrt/3d"

<window>
  <Scene width={720} height={720}>
    <PerspectiveCamera position={[0, 1.5, 3]} lookAt={[0, 0, 0]} />
    <Mesh geometry={box()} material={unlit({ color: [0.9, 0.3, 0.3] })} />
  </Scene>
</window>
```

To use it, select it in the scaffolder, or use `bun add @solidrt/3d`. The
API of this extension changes more frequently than the rest of SolidRT. For
an overview and the export surface, see [@solidrt/3d](/extensions/3d/).
