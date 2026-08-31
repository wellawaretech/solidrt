// @solidrt/3d - a retained 3D scene graph above @solidrt/core/gpu.
// Meshes, materials, and a camera compile to one depth-buffered draw
// target; the output is an ordinary texture id in the UI tree. Two layers:
// the imperative core (createScene/createMesh/setTransform - usable
// without Solid components) and the component face (Scene/Mesh/Group/
// PerspectiveCamera) on top. See AGENTS.md for the model and the traps.

export { add, createDirectionalLight, createGroup, createHemisphereLight, createInstancedMesh, createMesh, createScene, createSprite, setLight, disposeInstances, getRotation, lookAt, remove, setCastShadow, setGeometry, setInstanceCount, setInstances, setLayers, setMaterial, setMeshParams, setRenderOrder, setTransform, setTransition, setVisible, worldPosition, MAX_SHADOWS } from "./scene.ts"
export type { CameraUpdate, DirectionalLight as DirectionalLightNode, DirectionalLightOptions, FogOptions, HemisphereLight as HemisphereLightNode, HemisphereLightOptions, Hit, Light, InstancedMesh as InstancedMeshNode, InstancedMeshOptions, Mesh as MeshNode, MeshInstances, OrthoExtent, RaycastOptions, Scene as SceneHandle, SceneHandlers, SceneNode, SceneOptions, ScenePointerEvent, ScreenRay, ShadowCamera, ShadowOptions, TransformUpdate, TransitionEndEvent, View, ViewOptions } from "./scene.ts"
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
export { DirectionalLight, Group, HemisphereLight, InstancedMesh, Mesh, PerspectiveCamera, Scene, Sprite, useScene } from "./components.tsx"
export type { DirectionalLightProps, HemisphereLightProps, InstancedMeshProps, MeshProps, PerspectiveCameraProps, PointerEventProps, SceneProps, SpriteProps, TransformProps } from "./components.tsx"
export { gltfExternalUris, isGlb, parseGltf } from "./gltf.ts"
export type { ModelData, ModelMaterial, ModelPart, UriResolver } from "./gltf.ts"
export { decodeModel, encodeModel } from "./model-file.ts"
export { createModel, loadGltf, loadModel } from "./model.ts"
export type { Model, ModelMaps, ModelOptions } from "./model.ts"
export { createOrbitCamera } from "./orbit.ts"
export type { OrbitCamera, OrbitCameraOptions, OrbitPose, OrbitTarget } from "./orbit.ts"
// math's lookAt (the camera view matrix) stays on the /math subpath: the
// root's lookAt is the scene verb, the same split as `add`.
export { rayBoxDistance, compose, copy, eulerFromQuat, identity, mat4, multiply, normalMatrix, orthographic, perspective, quat, quatFromAxisAngle, quatFromEuler, quatFromFrame, quatFromTo, quatMultiply, quatNormalize, quatSlerp } from "./math.ts"
export type { Mat4, Quat, Vec2, Vec3 } from "./math.ts"
