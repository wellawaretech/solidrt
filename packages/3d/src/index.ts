// @solidrt/3d - a retained 3D scene graph above @solidrt/core/gpu.
// Meshes, materials, and a camera compile to one depth-buffered draw
// target; the output is an ordinary texture id in the UI tree. Two layers:
// the imperative core (createScene/createMesh/setTransform - usable
// without Solid components) and the component face (Scene/Mesh/Group/
// PerspectiveCamera) on top. See AGENTS.md for the model and the traps.

export { add, createGroup, getRotation, lookAt, remove, setTransform, setTransition, setVisible, worldPosition } from "./node.ts"
export type { SceneNode, ScenePointerEvent, TransformUpdate, TransitionEndEvent } from "./node.ts"
export { createInstancedMesh, createMesh, createSprite, disposeInstances, setCastShadow, setGeometry, setInstanceCount, setInstances, setLayers, setMaterial, setMeshParams, setRenderOrder } from "./mesh.ts"
export type { InstancedMesh as InstancedMeshNode, InstancedMeshOptions, Mesh as MeshNode, MeshInstances } from "./mesh.ts"
export { createDirectionalLight, createHemisphereLight, createPointLight, createSpotLight, setLight, MAX_SHADOWS } from "./light.ts"
export type {
  DirectionalLight as DirectionalLightNode,
  DirectionalLightOptions,
  HemisphereLight as HemisphereLightNode,
  HemisphereLightOptions,
  Light,
  PointLight as PointLightNode,
  PointLightOptions,
  CastingLight,
  ShadowCamera,
  ShadowOptions,
  SpotLight as SpotLightNode,
  SpotLightOptions,
  SpotShadowOptions,
} from "./light.ts"
export { createScene } from "./scene.ts"
export type { FogOptions, Hit, RaycastOptions, Scene as SceneHandle, SceneHandlers, SceneOptions, ScreenRay, View, ViewOptions } from "./scene.ts"
export type { CameraState, CameraUpdate, OrthoExtent } from "./camera.ts"
export type { NodeTransition, NodeTransitionSpec } from "flux:spatial"
export { disposeGeometry } from "./geometry-gpu.ts"
export { box, circle, cone, cylinder, fillAttribute, fillColors, geometryBounds, layoutAttributes, layoutKey, layoutSlot, layoutStride, mergeGeometries, packGeometry, plane, ring, sphere, torus, torusKnot, transformGeometry, validateGeometry, withAttribute, withColors, STANDARD_FLOATS, VERTEX_LAYOUTS } from "./geometry.ts"
export type { AttributeFill, BoxOptions, CircleOptions, ColorFill, ConeOptions, CylinderOptions, Geometry, GeometryOptions, PlaneOptions, RingOptions, SphereOptions, TorusKnotOptions, TorusOptions, VertexLayout } from "./geometry.ts"
export { fillet, roundRect, shape, triangulate } from "./profile.ts"
export type { Profile, ProfilePoint } from "./profile.ts"
export { extrude, lathe, pathFrames, sweep, tube } from "./sweep.ts"
export type { ExtrudeOptions, LatheOptions, PathFrames, PathPoint, SweepPath, TubeOptions } from "./sweep.ts"
export { lit, shaderMaterial, shaderMaterialClass, sprite, unlit } from "./material.ts"
export type { LitOptions, Material, ShaderMaterialClass, ShaderMaterialClassOptions, ShaderMaterialInstanceOptions, ShaderMaterialOptions, SpriteOptions, UnlitOptions } from "./material.ts"
export { DirectionalLight, Group, HemisphereLight, InstancedMesh, Mesh, OrbitCamera, PerspectiveCamera, PointLight, Scene, SpotLight, Sprite, useScene } from "./components.tsx"
export type {
  DirectionalLightProps,
  HemisphereLightProps,
  InstancedMeshProps,
  MeshProps,
  OrbitCameraProps,
  PerspectiveCameraProps,
  PointerEventProps,
  PointLightProps,
  SceneInput,
  SceneInputListener,
  SceneProps,
  SpotLightProps,
  SpriteProps,
  TransformProps,
} from "./components.tsx"
export { gltfExternalUris, isGlb, parseGltf } from "./gltf.ts"
export type { ModelChannel, ModelClip, ModelData, ModelMaterial, ModelNode, ModelPart, ModelSkin, UriResolver } from "./gltf.ts"
export { decodeModel, encodeModel } from "./model-file.ts"
export { createModel, loadGltf, loadModel } from "./model.ts"
export type { Model, ModelMaps, ModelOptions } from "./model.ts"
export { createMixer } from "./mixer.ts"
export type { Mixer, MixerPlayOptions } from "./mixer.ts"
export { sampleChannel } from "./clip.ts"
export { createOrbitCamera } from "./orbit.ts"
export type { OrbitCamera as OrbitCameraHandle, OrbitCameraOptions, OrbitPose, OrbitTarget } from "./orbit.ts"
// math's lookAt (the camera view matrix) stays on the /math subpath: the
// root's lookAt is the scene verb, the same split as `add`.
export { rayBoxDistance, compose, copy, eulerFromQuat, identity, mat4, multiply, normalMatrix, orthographic, perspective, quat, quatFromAxisAngle, quatFromEuler, quatFromFrame, quatFromTo, quatMultiply, quatNormalize, quatSlerp } from "./math.ts"
export type { Mat4, Quat, Vec2, Vec3 } from "./math.ts"
