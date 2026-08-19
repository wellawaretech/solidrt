// @solidrt/2d - an instanced sprite layer above @solidrt/core/gpu.
// One atlas, one instance buffer, N quads in one draw; sprite records are
// published through the zero-copy buffer write lease, so per-frame motion
// costs float stores plus one bulk publish, never per-sprite property
// writes. Two layers: the imperative core (createSpriteLayer/addSprite/
// setSprite - usable without Solid components) and the component face
// (SpriteLayer/Sprite) on top. See AGENTS.md for the model and the traps.

export { addSprite, createSpriteLayer, getSprite, removeSprite, setSprite, FLOATS_PER_SPRITE } from "./layer.ts"
export { pointInSprite } from "./pick.ts"
export type { CameraUpdate, Sprite as SpriteHandle, SpriteHandlers, SpriteLayer as SpriteLayerHandle, SpriteLayerOptions, SpriteOptions, SpritePointerEvent } from "./layer.ts"
export { grid, namedFrames, FULL_FRAME } from "./frames.ts"
export type { Frame, GridOptions } from "./frames.ts"
export { createAtlas } from "./atlas.ts"
export type { Atlas, AtlasOptions } from "./atlas.ts"
export { Sprite, SpriteLayer, useSpriteLayer } from "./components.tsx"
export type { SpriteLayerProps, SpritePointerProps, SpriteProps } from "./components.tsx"
