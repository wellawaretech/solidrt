// @solidrt/3d - a retained 3D scene graph above @solidrt/core/gpu.
// Meshes, materials, and a camera compile to one depth-buffered draw
// target; the output is an ordinary texture id in the UI tree. Two layers:
// the imperative core (createScene/createMesh/setTransform - usable
// without Solid components) and the component face (Scene/Mesh/Group/
// PerspectiveCamera) on top. See AGENTS.md for the model and the traps.

export { add, createGroup, createMesh, createScene, remove, setGeometry, setMaterial, setMeshParams, setTransform, setVisible } from "./scene.ts"
export type { CameraUpdate, Mesh as MeshNode, Scene as SceneHandle, SceneNode, SceneOptions, TransformUpdate } from "./scene.ts"
export { box, disposeGeometry, plane, sphere, FLOATS_PER_VERTEX, VERTEX_LAYOUT } from "./geometry.ts"
export type { Geometry } from "./geometry.ts"
export { shaderMaterial, unlit } from "./material.ts"
export type { Material, ShaderMaterialOptions, UnlitOptions } from "./material.ts"
export { Group, Mesh, PerspectiveCamera, Scene, useScene } from "./components.tsx"
export type { MeshProps, PerspectiveCameraProps, SceneProps, TransformProps } from "./components.tsx"
export { compose, copy, identity, lookAt, mat4, multiply, perspective } from "./math.ts"
export type { Mat4, Vec3 } from "./math.ts"
