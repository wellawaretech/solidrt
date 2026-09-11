// The Solid face: PascalCase components over context, syncing the retained
// scene (node/mesh/light/scene.ts) - no new intrinsic elements, no
// renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained nodes and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// node with `ref` and call setTransform from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the scene.
//
// One file per component (the lights, cameras and meshes each their own),
// with the shared pieces beside them: context.tsx (the scene context and
// useScene), node-props.ts (the transform and pointer
// props every node component syncs) and mesh.tsx (the mesh-side props the
// populated meshes share). This index is the package's face: index.ts at
// the root re-exports it, nothing outside imports the files directly.

export { useScene } from "./context.tsx"
export type { CameraTarget } from "./context.tsx"
export type { PointerEventProps, TransformProps } from "./node-props.ts"
export { Scene } from "./scene.tsx"
export type { ScenePointerProps, SceneProps } from "./scene.tsx"
export { View3d } from "./view3d.tsx"
export type { View3dProps } from "./view3d.tsx"
export { Group } from "./group.tsx"
export { Mesh } from "./mesh.tsx"
export type { MeshProps } from "./mesh.tsx"
export { Sprite } from "./sprite.tsx"
export type { SpriteProps } from "./sprite.tsx"
export { Instance, InstancedMesh } from "./instanced-mesh.tsx"
export type { InstancedMeshProps, InstanceProps } from "./instanced-mesh.tsx"
export { InstancedLod, Lod } from "./lod.tsx"
export type { InstancedLodProps, LodProps } from "./lod.tsx"
export { RecordMesh } from "./record-mesh.tsx"
export type { RecordMeshProps } from "./record-mesh.tsx"
export { PerspectiveCamera } from "./perspective-camera.tsx"
export type { PerspectiveCameraProps } from "./perspective-camera.tsx"
export { OrbitCamera } from "./orbit-camera.tsx"
export type { OrbitCameraProps } from "./orbit-camera.tsx"
export { FirstPersonCamera } from "./first-person-camera.tsx"
export type { FirstPersonCameraProps } from "./first-person-camera.tsx"
export { HemisphereLight } from "./hemisphere-light.tsx"
export type { HemisphereLightProps } from "./hemisphere-light.tsx"
export { DirectionalLight } from "./directional-light.tsx"
export type { DirectionalLightProps } from "./directional-light.tsx"
export { SpotLight } from "./spot-light.tsx"
export type { SpotLightProps } from "./spot-light.tsx"
export { PointLight } from "./point-light.tsx"
export type { PointLightProps } from "./point-light.tsx"
