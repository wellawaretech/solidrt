// @solidrt/2d - an instanced sprite layer above @solidrt/core/gpu.
// One atlas, N quads in one draw per VIEW: a layer holds the sprites and
// shows only through its views (layer.createView / <View2d>, each a target
// with a camera of its own - a Unity scene renders only through Cameras,
// a Godot World2D only through Viewports). The live layer
// (createSpriteLayer/addSprite) backs every sprite with a SPATIAL ARENA
// node whose Pose2D record sink writes the pose instance buffer at the
// core flush, so core producers reach sprites and picking walks the core
// BVH; style stays a JS-written second instance buffer. The records layer (createRecordLayer)
// is the raw escape hatch for motion only JS can compute: 13 JS-owned
// floats per sprite published through the zero-copy write lease. The baked
// tile layer (createTileLayer/TileLayer) is the static sibling: a tile
// world rendered once into textures and composited as a few quads,
// re-baked on change. Two faces throughout: the imperative core (usable
// without Solid components) and the components (SpriteLayer/Sprite/Group/
// View2d/TileLayer) on top. See AGENTS.md for the model and the traps.

export { addGroup, addSprite, createSpriteLayer, getSprite, removeGroup, removeSprite, setGroup, setGroupTransition, setSprite, setSpriteParent, setSpriteTransition, worldPosition, POSE_FLOATS, STYLE_FLOATS } from "./layer.ts"
export { createRecordLayer, FLOATS_PER_SPRITE } from "./records.ts"
export type { RecordLayer as RecordLayerHandle, RecordLayerOptions } from "./records.ts"
export { pointInSprite } from "./pick.ts"
export { projectCamera, unprojectCamera } from "./camera.ts"
export type { CameraState, CameraUpdate } from "./camera.ts"
export { createCamera2d } from "./camera2d.ts"
export type { Camera2d as Camera2dHandle, Camera2dAxes, Camera2dOptions, Camera2dPose, Camera2dTarget, Rect2d } from "./camera2d.ts"
export { camera2dActions, camera2dBindings } from "./input.ts"
export type { CameraDevices } from "./input.ts"
export type {
  AddSpriteOptions,
  Capsule,
  Circle,
  GroupOptions,
  Impact,
  MoveOptions,
  MoveResult,
  Overlap,
  QueryOptions,
  RayHit,
  Rect,
  LayerPointerEvent,
  LayerPointerListener,
  LayerTapEvent,
  LayerWheelEvent,
  Sprite as SpriteHandle,
  SpriteGroup,
  SpriteHandlers,
  SpriteLayer as SpriteLayerHandle,
  SpriteLayerOptions,
  SpriteOptions,
  SpritePointerEvent,
  SpriteTapEvent,
  SpriteWheelEvent,
  TransitionEndEvent,
  Volume,
} from "./layer.ts"
export { feedPointer } from "./views.ts"
export type { ViewHandle, ViewOptions } from "./views.ts"
export { createTileLayer } from "./tiles.ts"
export type { TileChunk, TileLayer as TileLayerHandle, TileLayerOptions } from "./tiles.ts"

export type { NodeTransition, NodeTransitionSpec } from "flux:spatial"
export { grid, namedFrames, FULL_FRAME } from "./frames.ts"
export type { AtlasSize, Frame, GridOptions, NamedFramesOptions } from "./frames.ts"
export { createAnimation } from "./animation.ts"
export type { AnimationOptions, SpriteAnimation } from "./animation.ts"
export { fitOversample } from "./oversample.ts"
export { createAtlas } from "./atlas.ts"
export type { Atlas, AtlasOptions } from "./atlas.ts"
export { Camera2d, Group, Sprite, SpriteLayer, TileLayer, View2d, useSpriteLayer } from "./components/index.ts"
export type { Camera2dProps, GroupProps, LayerPointerProps, SpriteLayerProps, SpritePointerProps, SpriteProps, TileCamera, TileLayerProps, View2dProps } from "./components/index.ts"
