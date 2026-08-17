// @solidrt/3d - a retained 3D scene graph above @solidrt/core/gpu.
// Meshes, materials, and a camera compile to one depth-buffered draw
// target; the output is an ordinary texture id in the UI tree. Two layers:
// the imperative core (createScene/createMesh/setTransform - usable
// without Solid components) and the component face (Scene/Mesh/Group/
// PerspectiveCamera) on top. See AGENTS.md for the model and the traps.

export { add, createGroup, createMesh, createScene, getRotation, lookAt, remove, setGeometry, setMaterial, setMeshParams, setRenderOrder, setTransform, setVisible, worldPosition } from "./scene.ts"
export type { CameraUpdate, Hit, Mesh as MeshNode, Scene as SceneHandle, SceneHandlers, SceneNode, SceneOptions, ScenePointerEvent, TransformUpdate } from "./scene.ts"
export { box, circle, cone, cylinder, disposeGeometry, fillColors, plane, ring, sphere, torus, torusKnot, withColors, FLOATS_PER_VERTEX, VERTEX_LAYOUTS } from "./geometry.ts"
export type { ColorFill, Geometry, VertexLayout } from "./geometry.ts"
export { fillet, roundRect, shape, triangulate } from "./profile.ts"
export type { Profile, ProfilePoint } from "./profile.ts"
export { extrude, lathe, pathFrames, sweep, tube } from "./sweep.ts"
export type { PathFrames, PathPoint, SweepPath } from "./sweep.ts"
export { shaderMaterial, shaderMaterialClass, unlit } from "./material.ts"
export type { Material, ShaderMaterialClass, ShaderMaterialClassOptions, ShaderMaterialInstanceOptions, ShaderMaterialOptions, UnlitOptions } from "./material.ts"
export { Group, Mesh, PerspectiveCamera, Scene, useScene } from "./components.tsx"
export type { MeshProps, PerspectiveCameraProps, PointerEventProps, SceneProps, TransformProps } from "./components.tsx"
export { createOrbitCamera } from "./orbit.ts"
export type { OrbitCamera, OrbitCameraOptions, OrbitPose } from "./orbit.ts"
// math's lookAt (the camera view matrix) stays on the /math subpath: the
// root's lookAt is the scene verb, the same split as `add`.
export { compose, copy, eulerFromQuat, identity, mat4, multiply, normalMatrix, perspective, quat, quatFromAxisAngle, quatFromEuler, quatFromFrame, quatFromTo, quatMultiply, quatNormalize, quatSlerp } from "./math.ts"
export type { Mat4, Quat, Vec2, Vec3 } from "./math.ts"
