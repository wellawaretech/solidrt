// @solidrt/3d - a retained 3D scene graph above @solidrt/core/gpu.
// Meshes, materials, and a camera compile to one depth-buffered draw
// target; the output is an ordinary texture id in the UI tree. Two layers:
// the imperative core (createScene/createMesh/setTransform - usable
// without Solid components) and the component face (Scene/Mesh/Group/
// PerspectiveCamera) on top. See AGENTS.md for the model and the traps.

export { add, createGroup, destroy, getMorphNames, getMorphWeights, getRotation, getTransform, lookAt, remove, setMorphWeights, setTimeScale, setTransform, setTransition, setVisible, timeRate, worldInto, worldPosition } from "./node.ts"
export type { HoverHandlers, LodConfig, MorphWeights, NodePointerEvent, NodeTapEvent, NodeWheelEvent, SceneEventBase, SceneNode, ScenePointerEvent, ScenePointerListener, SceneTapEvent, SceneWheelEvent, TransformUpdate, TransitionEndEvent } from "./node.ts"
export { addInstance, createInstancedMesh, createMesh, createRecordMesh, createSprite, disposeInstances, setCastShadow, setCulling, setDrawRange, setEnvironment, setGeometry, setInstanceStyle, setLayers, setMaterial, setMeshParams, setRecordCount, setRecordLod, setRecords, setRenderOrder, instanceAttribute, records, updateRecords, INSTANCE_FLOATS } from "./mesh.ts"
export type { InstancedMesh as InstancedMeshNode, InstancedMeshOptions, InstanceNode, InstanceOrderOptions, InstanceSlots, InstanceStream, Mesh as MeshNode, MeshInstances, RecordMesh as RecordMeshNode, RecordMeshOptions, UpdateRecordsOptions } from "./mesh.ts"
export { createDirectionalLight, createHemisphereLight, createPointLight, createSpotLight, setLight } from "./light.ts"
// The shader-source caps, also on the /glsl subpath next to the sources they size.
export { MAX_CASCADES, MAX_LIGHTS, MAX_SHADOW_MAPS, MAX_SHADOW_TAPS } from "./glsl.ts"
export type { ShadowQuality } from "./scene-shadows.ts"
export { DEFAULT_SHADOW_CAMERA } from "./light.ts"
// The quality table every knob defaults from (okf/done/3d-quality-tier.md).
export { QUALITY_TIERS, resolveQuality } from "./quality.ts"
export type { QualityInput, QualityRow } from "./quality.ts"
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
export { createInstancedLod, createLod, setLod } from "./lod.ts"
export type { InstancedLodLevel, InstancedLodOptions, LodLevel, LodOptions } from "./lod.ts"
export { createScene, resolveParams } from "./scene.ts"
export { feedPointer } from "./scene-pointer.ts"
export type { BlendSpace, BloomOptions, Capsule, EnvironmentOptions, FogOptions, Hit, Impact, MoveOptions, MoveResult, OrientedBox, Overlap, QueryOptions, ReflectionProbe, ReflectionProbeOptions, ResolveInput, ResolveOptions, Scene as SceneHandle, SceneHandlers, SceneOptions, ScreenRay, SkyboxOptions, Sphere, ToneMapping, ViewHandle, ViewOptions, Volume } from "./scene.ts"
export { moveAndSlide } from "./collision.ts"
export type { MoveScene } from "./collision.ts"
export { bufferFormat, equirectToCube, loadEnvironment } from "./environment.ts"
export type { BufferFormat } from "./environment.ts"
export { linearColor, linearToSrgb, premultipliedColor, srgbToLinear } from "./color.ts"
export type { CameraState, CameraUpdate, OrthoExtent } from "./camera.ts"
export type { NodeMotionSpec, NodeTransition, NodeTransitionSpec } from "flux:spatial"
export { computeVertexNormals, disposeGeometry, updateVertices } from "./geometry-gpu.ts"
export type { GeometryBuffers, UpdateVerticesOptions } from "./geometry-gpu.ts"
export { arrowHelper, axesHelper, box, box3Helper, capsule, capsuleHelper, circle, cone, cylinder, dodecahedron, fillAttribute, fillColors, edgesGeometry, mergeVertices, normalsHelper, toNonIndexed, withNormals, geometryBounds, geometryTopology, gridHelper, icosahedron, attributeAccess, geometryAttribute, geometryKey, geometryLayouts, geometrySlot, geometryStreams, geometryVertexCount, isFloatFormat, isFloatLayout, layoutAttributes, layoutKey, layoutSlot, layoutStride, mergeGeometries, octahedron, packGeometry, packMorphTargets, plane, planeHelper, polyhedron, ring, sphere, tetrahedron, torus, torusKnot, transformGeometry, validateGeometry, wireframeGeometry, vertexBytes, vertexCount, vertexView, withAttribute, withColors, withMorphTargets, MORPH_ENTRY_TEXELS, MORPH_TEXEL_FLOATS, BASE_FLOATS, VERTEX_FORMATS, VERTEX_LAYOUTS } from "./geometry.ts"
export type { ArrowHelperOptions, AttributeAccess, AttributeFill, VertexStream, WithAttributeOptions, AxesHelperOptions, BoxOptions, CapsuleHelperOptions, CapsuleOptions, CircleOptions, ColorFill, ConeOptions, CylinderOptions, FormatCodec, Geometry, GeometryOptions, GridHelperOptions, MorphTarget, MorphTargets, NormalsHelperOptions, PlaneHelperOptions, PlaneOptions, PolyhedronOptions, RingOptions, SphereOptions, TorusKnotOptions, TorusOptions, VertexLayout } from "./geometry.ts"
export { fillet, roundRect, triangulate } from "./profile.ts"
export type { Profile, ProfilePoint } from "./profile.ts"
export { extrude, lathe, pathFrames, polygon, sweep, tube } from "./sweep.ts"
export type { ExtrudeOptions, LatheOptions, PathFrames, PathPoint, SweepPath, TubeOptions } from "./sweep.ts"
export { phong, shaderMaterial, shaderMaterialClass, sprite, standard, unlit } from "./material.ts"
export type { LitOptions, Material, PhongOptions, ShaderMaterialClass, ShaderMaterialClassOptions, ShaderMaterialInstanceOptions, ShaderMaterialOptions, SpriteOptions, StandardOptions, UnlitOptions } from "./material.ts"
export { DirectionalLight, FirstPersonCamera, Group, HemisphereLight, Instance, InstancedLod, InstancedMesh, Lod, Mesh, OrbitCamera, PerspectiveCamera, PointLight, RecordMesh, Region, RegionGrid, Scene, SplatMesh, SpotLight, Sprite, View3d, useScene, Shot, Shots } from "./components/index.ts"
export type {
  CameraTarget,
  DensityProps,
  DirectionalLightProps,
  FirstPersonCameraProps,
  HemisphereLightProps,
  InstancedLodProps,
  InstancedMeshProps,
  InstanceProps,
  LodProps,
  RegionGridProps,
  RegionProps,
  MeshProps,
  OrbitCameraProps,
  PerspectiveCameraProps,
  NodePointerProps,
  BubblingNodePointerProps,
  GroupProps,
  PointLightProps,
  RecordMeshProps,
  ScenePointerProps,
  SceneProps,
  SplatMeshProps,
  SpotLightProps,
  SpriteProps,
  TransformProps,
  View3dProps,
  OutputLeaf,
  ShotProps,
  ShotsProps,
} from "./components/index.ts"
export { gltfExternalUris, isGlb, modelImageUses, parseGltf } from "./gltf.ts"
export type { ModelChannel, ModelClip, ModelCurve, ModelData, ModelExtras, ModelImage, ModelImageUse, ModelMarker, ModelMaterial, ModelNode, ModelPart, ModelSkin, ModelTexture, UriResolver } from "./gltf.ts"
export { decodeModel, encodeModel } from "./model-file.ts"
export { createModel, loadGltf, loadModel, transcodeModelImages } from "./model.ts"
export type { Model, ModelMaps, ModelOptions } from "./model.ts"
export {
  decodeRecord,
  decodeSh,
  decodeSplat,
  encodeSplat,
  parseSplat,
  splatChunkBounds,
  SPLAT_ATTRIBUTES,
  SPLAT_CHUNK_BYTES,
  SPLAT_CHUNK_FLOATS,
  SPLAT_CHUNK_LIMIT,
  SPLAT_CHUNK_MAX,
  SPLAT_CHUNK_MIN,
  SPLAT_RAMP_BYTES,
  SPLAT_RAMP_SAMPLES,
  SPLAT_RUN_BYTES,
  SPLAT_CHUNK_TEXELS,
  SPLAT_COLOR_BYTES,
  SPLAT_POSITION_BITS,
  SPLAT_RECORD_BYTES,
  SPLAT_RECORD_TEXELS,
  SPLAT_ROTATION_BITS,
  SPLAT_SCALE_BITS,
  SPLAT_SH_BYTE_SCALE,
  SPLAT_SH_BYTE_ZERO,
  SPLAT_SH_LABEL_BYTES,
  SPLAT_SH_LABEL_TEXEL_BYTES,
  SPLAT_SH_LABEL_TEXELS,
  SPLAT_SH_PALETTE_SIZE,
  SPLAT_SH_SUBSPACES,
  SPLAT_SH_TEXELS,
  SPLAT_STEP_VISIBLE,
  SPLAT_TEXEL_BYTES,
} from "./splat-data.ts"
export type { BakeReport, ParseSplatOptions, SplatData, SplatFormat, SplatRecord, SplatShEncoding, SplatShSubspace } from "./splat-data.ts"
export { createSplatMesh, loadSplat, splatMaterialClass, splatVertexSource, SPLAT_FRAGMENT, SPLAT_GROUP, SPLAT_ID_ATTRIBUTES, SPLAT_LOD_THRESHOLD } from "./splat.ts"
export type { SplatMaterialOptions, SplatMesh as SplatMeshNode, SplatMeshOptions } from "./splat.ts"
export { bindSkeleton } from "./skeleton.ts"
export type { BindSkeletonOptions } from "./skeleton.ts"
export { createMixer } from "./mixer.ts"
export type { AdditiveReferenceSpec, BlendKind, LayerOptions, Marker, MarkerEvent, MaskEntry, MaskSpec, Mixer, MixerOptions, MixerPlayOptions, RigSpec, RootMotionMode } from "./mixer.ts"
export { createBlendSpace } from "./blend-space.ts"
export type { ClipBlendSample, ClipBlendSpace, ClipBlendSpaceOptions } from "./blend-space.ts"
export { createSpringBones } from "./springs.ts"
export type { SpringBones, SpringBonesOptions, SpringChainSpec, SpringColliderSpec, SpringJointSpec, SpringJointUpdate } from "./springs.ts"
export { createIk } from "./ik.ts"
export { occludersFromMeshes, setOccluder } from "./occluder.ts"
export { attachSound } from "./sound.ts"
export type { PositionalSoundOptions } from "./sound.ts"
export type { CullState, OccluderSpec } from "./occluder.ts"
export type { Ik, IkChainSpec, IkCurveWeight, IkFeetSpec, IkLimitSpec, IkLookAtSpec, IkOptions, IkPointSpec, IkSolverSpec, IkSolverUpdate, IkTargetSpec, IkTwoBoneSpec, IkUpSpec } from "./ik.ts"
export { channelElements, sampleChannel } from "./clip.ts"
export { createOrbitCamera } from "./orbit.ts"
export type { OrbitAxes, OrbitCamera as OrbitCameraHandle, OrbitCameraOptions, OrbitPose, OrbitPoseState, OrbitTarget } from "./orbit.ts"
export { createFirstPersonCamera } from "./first-person.ts"
export type { FirstPersonAxes, FirstPersonCamera as FirstPersonCameraHandle, FirstPersonCameraOptions, FirstPersonPose, FirstPersonPoseState, FirstPersonTarget } from "./first-person.ts"
export { createShots, mixCamera } from "./shots.ts"
export type { ShotsHandle, ShotsOwner, ShotTarget } from "./shots.ts"
export { firstPersonActions, firstPersonBindings, mapBindings, orbitActions, orbitBindings } from "./input.ts"
export type { CameraDevices } from "./input.ts"
// math's lookAt (the camera view matrix) stays on the /math subpath: the
// root's lookAt is the scene verb, the same split as `add`.
export { rayBoxDistance, compose, copy, eulerFromQuat, identity, mat4, multiply, normalMatrix, orthographic, perspective, quat, quatFromAxisAngle, quatFromEuler, quatFromFrame, quatFromTo, quatMultiply, quatNormalize, quatSlerp } from "./math.ts"
export type { Mat4, Quat, Vec2, Vec3 } from "./math.ts"
export { buildModel, createRegion, createRegionGrid, createStreamingSource, geometryBytes } from "./region.ts"
export type { RegionContent, RegionGridNode, RegionGridOptions, RegionNode, RegionOptions, StreamingSource } from "./region.ts"
export type { RegionCell, RegionState } from "./region.ts"
export { setResourcePool, resourcePool } from "./resource-pool.ts"
export { setResidencyBudget, setResidencyLimit } from "@solidrt/core"
export { modelSteps } from "./model.ts"
