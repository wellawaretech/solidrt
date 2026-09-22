// @solidrt/3d/model - the model DATA side of the package without the
// runtime: the glTF parser, the .srtm container and the pure geometry kit,
// for a script that bakes an app's own geometry under bun (or any host
// with no flux). The package root imports `flux:*` and so loads only on
// the runtime; this entry imports nothing that does. tools/model.ts (the
// glTF bake) runs on exactly this surface, so an app's bake script and
// the tool share one path:
//
//   import { box, encodeModel, transformGeometry } from "@solidrt/3d/model"
//   writeFileSync("assets/terrain.srtm", encodeModel({ nodes, parts, ... }))
//
// createModel / loadModel on the runtime read what it writes. Keep this
// entry pure: tests/model-data.test.ts imports it under bun and fails the
// moment a runtime import creeps into the chain.

export { gltfExternalUris, isGlb, parseGltf } from "./gltf.ts"
export type { ModelChannel, ModelClip, ModelData, ModelExtras, ModelMaterial, ModelNode, ModelPart, ModelSkin, UriResolver } from "./gltf.ts"
export { decodeModel, encodeModel } from "./model-file.ts"
export { arrowHelper, axesHelper, box, box3Helper, capsule, capsuleHelper, circle, cone, cylinder, dodecahedron, fillAttribute, fillColors, edgesGeometry, mergeVertices, normalsHelper, toNonIndexed, withNormals, geometryBounds, geometryTopology, gridHelper, icosahedron, attributeAccess, geometryAttribute, geometryKey, geometryLayouts, geometrySlot, geometryStreams, geometryVertexCount, isFloatFormat, isFloatLayout, layoutAttributes, layoutKey, layoutSlot, layoutStride, mergeGeometries, octahedron, packGeometry, packMorphTargets, plane, planeHelper, polyhedron, ring, sphere, tetrahedron, torus, torusKnot, transformGeometry, validateGeometry, wireframeGeometry, vertexBytes, vertexCount, vertexView, withAttribute, withColors, withMorphTargets, MORPH_ENTRY_TEXELS, MORPH_TEXEL_FLOATS, BASE_FLOATS, VERTEX_FORMATS, VERTEX_LAYOUTS } from "./geometry.ts"
export type { ArrowHelperOptions, AttributeAccess, AttributeFill, VertexStream, WithAttributeOptions, AxesHelperOptions, BoxOptions, CapsuleHelperOptions, CapsuleOptions, CircleOptions, ColorFill, ConeOptions, CylinderOptions, FormatCodec, Geometry, GeometryOptions, GridHelperOptions, MorphTarget, MorphTargets, NormalsHelperOptions, PlaneHelperOptions, PlaneOptions, PolyhedronOptions, RingOptions, SphereOptions, TorusKnotOptions, TorusOptions, VertexLayout } from "./geometry.ts"
export { fillet, roundRect, triangulate } from "./profile.ts"
export type { Profile, ProfilePoint } from "./profile.ts"
export { extrude, lathe, pathFrames, polygon, sweep, tube } from "./sweep.ts"
export type { ExtrudeOptions, LatheOptions, PathFrames, PathPoint, SweepPath, TubeOptions } from "./sweep.ts"
export { linearColor, linearToSrgb, premultipliedColor, srgbToLinear } from "./color.ts"
export { rayBoxDistance, compose, copy, eulerFromQuat, identity, lookAt, mat4, multiply, normalMatrix, orthographic, perspective, quat, quatFromAxisAngle, quatFromEuler, quatFromFrame, quatFromTo, quatMultiply, quatNormalize, quatSlerp } from "./math.ts"
export type { Mat4, Quat, Vec2, Vec3 } from "./math.ts"
