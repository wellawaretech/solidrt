// The Solid face: PascalCase components over context, syncing the retained
// layer (layer.ts) - no new intrinsic elements, no renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained records and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// sprite with `ref` and call setSprite from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the layer.
// The same split, with the same reasoning, as @solidrt/3d's components.
//
// One file per component, with the shared pieces beside them: context.ts
// (the layer, group and viewport contexts and useSpriteLayer) and
// auto-oversample.ts (the measured auto-pick the leaves apply). This index is the package's
// face: index.ts at the root re-exports it, nothing outside imports the
// files directly.

export { useSpriteLayer } from "./context.ts"
export { SpriteLayer } from "./sprite-layer.tsx"
export type { LayerPointerProps, SpriteLayerProps } from "./sprite-layer.tsx"
export { Group } from "./group.tsx"
export type { GroupProps } from "./group.tsx"
export { Sprite } from "./sprite.tsx"
export type { SpritePointerProps, SpriteProps } from "./sprite.tsx"
export { Camera2d } from "./camera2d.tsx"
export type { Camera2dProps } from "./camera2d.tsx"
export { Shot, Shots } from "./shots.tsx"
export type { ShotProps, ShotsProps } from "./shots.tsx"
export { View2d } from "./view2d.tsx"
export type { View2dProps } from "./view2d.tsx"
export { TileLayer } from "./tile-layer.tsx"
export type { TileCamera, TileLayerProps } from "./tile-layer.tsx"
