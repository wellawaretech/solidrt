// @solidrt/2d - an instanced sprite layer above @solidrt/core/gpu.
// A layer's atlases bound together, N quads in one draw per VIEW: a layer
// holds the sprites and shows only through its views (layer.createView /
// <View2d>, each a target with a camera of its own - a Unity scene renders
// only through Cameras, a Godot World2D only through Viewports); every
// frame names its texture, so sprites from several sheets interleave in
// the one draw (the PixiJS/Phaser multi-texture batch). The live layer
// (createSpriteLayer/addSprite) backs every sprite with a SPATIAL ARENA
// node whose Pose2D record sink writes the pose instance buffer at the
// core flush, so core producers reach sprites and picking walks the core
// BVH; style stays a JS-written second instance buffer. The records layer (createRecordLayer)
// is the raw escape hatch for motion only JS can compute: 16 JS-owned
// floats per sprite in a mirror, published by dirty range through the
// 3d record mesh's verbs (records/updateRecords/setRecordCount). The baked
// tile layer (createTileLayer/TileLayer) is the static sibling: a tile
// world rendered once into textures and composited as a few quads,
// re-baked on change. Two faces throughout: the imperative core (usable
// without Solid components) and the components (SpriteLayer/Sprite/Group/
// View2d/TileLayer) on top. Sheets reach the GPU through createAtlas,
// sliced by grid/namedFrames/fullFrame, and a mipmapped sheet is extruded
// first (extrudeGrid/extrudeRects). See AGENTS.md for the model and the
// traps.

export { addGroup, addSprite, createSpriteLayer, getSprite, destroyGroup, destroySprite, setGroup, setGroupTimeScale, setGroupTransition, setSprite, setSpriteParent, setSpriteTimeScale, setSpriteTransition, timeRate, worldPosition, POSE_FLOATS, STYLE_FLOATS } from "./layer.ts"
export { createRecordLayer, records, setRecordCount, updateRecords, INSTANCE_FLOATS } from "./records.ts"
export type { RecordLayer as RecordLayerHandle, RecordLayerOptions, UpdateRecordsOptions } from "./records.ts"
export { floorReach, pointInSprite } from "./pick.ts"
export { projectCamera, unprojectCamera } from "./camera.ts"
export type { CameraState, CameraUpdate } from "./camera.ts"
export { createCamera2d } from "./camera2d.ts"
export type { Camera2d as Camera2dHandle, Camera2dAxes, Camera2dOptions, Camera2dPose, Camera2dPoseState, Camera2dTarget, Rect2d } from "./camera2d.ts"
export { createShots, mixCamera } from "./shots.ts"
export type { ShotTarget, ShotsHandle } from "./shots.ts"
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
  Hit,
  Rect,
  LayerEventBase,
  LayerPointerEvent,
  LayerPointerListener,
  LayerTapEvent,
  LayerWheelEvent,
  Sprite as SpriteHandle,
  SpriteGroup,
  LayerHandlers,
  SpriteLayer as SpriteLayerHandle,
  SpriteLayerOptions,
  SpriteOptions,
  SpritePointerEvent,
  SpriteTapEvent,
  SpriteEndpoint,
  SpriteTransition,
  SpriteTransitionSpec,
  SpriteWheelEvent,
  TransitionEndEvent,
  Volume,
} from "./layer.ts"
export { feedPointer } from "./views.ts"
export { moveAndSlide } from "./collision.ts"
export type { MoveLayer } from "./collision.ts"
export type { ViewHandle, ViewOptions } from "./views.ts"
export { createTileLayer } from "./tiles.ts"
export type { TileChunk, TileLayer as TileLayerHandle, TileLayerOptions } from "./tiles.ts"

export type { NodeMotionSpec, NodeTransitionSpec } from "flux:spatial"
export { fullFrame, grid, isFrame, namedFrames } from "./frames.ts"
export type { Frame, GridOptions } from "./frames.ts"
export { extrudeGrid, extrudeRects } from "./extrude.ts"
export type { ExtrudedGrid, ExtrudedRects, Rects } from "./extrude.ts"
export { packRects, packWidth } from "./pack.ts"
export type { Packing, Placement, Size } from "./pack.ts"
export { createAnimation } from "./animation.ts"
export type { AnimationOptions, SpriteAnimation } from "./animation.ts"
export { fitOversample } from "./oversample.ts"
export { createAtlas, isAtlas } from "./atlas.ts"
export type { Atlas, AtlasOptions } from "./atlas.ts"
export { createSpriteFont } from "./font.ts"
export type { FontListener, Glyph, SpriteFont, SpriteFontOptions } from "./font.ts"
export { layoutText } from "./text-layout.ts"
export type { GlyphPlacement, TextAlign, TextAnchor, TextAnchorY, TextLayout, TextLayoutOptions, TextLine } from "./text-layout.ts"
export { addText, destroyText, setText } from "./text.ts"
export type { AddTextOptions, TextPose, TextRun, TextStyle } from "./text.ts"
export { Camera2d, Group, Sprite, SpriteLayer, Text2d, TileLayer, View2d, useSpriteLayer, Shot, Shots } from "./components/index.ts"
export type { BubblingSpritePointerProps, Camera2dProps, GroupProps, LayerPointerProps, SpriteLayerProps, SpritePointerProps, SpriteProps, TileCamera, TileLayerProps, View2dProps, ShotProps, ShotsProps } from "./components/index.ts"
