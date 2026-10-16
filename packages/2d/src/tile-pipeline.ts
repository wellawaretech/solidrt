// The chunk-quad pipeline a tile VIEW draws its resident chunks with: one
// entry per chunk image, a unit quad placed at the image's rect under the
// view's camera (the projectCamera mapping, camera.ts, spelled in GLSL
// like the sprite stage in glsl.ts) and the chunk's baked target sampled
// across it. One program, pipeline and quad for every tile layer and
// view in the process, made on first use and kept (a material class
// keeps its program the same way).
//
// Precision: a chunk's world rect can sit 10^7 px from the origin, past
// what a float32 subtraction keeps whole. The camera is therefore handed
// to the stage SPLIT: `uOrigin` is the chunk the camera point lies in
// (exact integers) and `uCamera.xy` the remainder inside that chunk in
// world px, while each entry carries its image's chunk indices in
// `uChunk`. The stage subtracts chunk indices (small exact integers)
// before scaling to pixels, so every quantity it adds is small, and a
// world 10^7 px out renders as crisp as one at the origin - the same
// subtraction the record bake does with chunk-local records.
import { compileShader, createBuffer, createRenderPipeline, destroyShader, glsl, linkProgram } from "@solidrt/core/gpu"
import type { BufferId, RenderPipelineId, ShaderParams } from "@solidrt/core/gpu"
import type { CameraState } from "./camera.ts"
import { floorDiv } from "./tiles-math.ts"

/** The view's sampler binding for an entry's chunk target. */
export const CHUNK_SAMPLER = "uChunkTex"

const TILE_VERTEX = glsl`
  in vec2 aPos;
  uniform vec2 uViewport;
  // [x, y, zoom, zoom]: the camera point minus its chunk's corner, world px.
  uniform vec4 uCamera;
  // [cos, sin, pivotX, pivotY]: the camera rotation about the pivot.
  uniform vec4 uCameraRot;
  // The chunk the camera point lies in, chunk indices.
  uniform vec2 uOrigin;
  // This image's chunk indices.
  uniform vec2 uChunk;
  // [chunkW, chunkH, margin, 0]: a chunk's world size and the overhang
  // margin its target carries on every side.
  uniform vec4 uChunkRect;
  out vec2 vUv;
  void main() {
    vec2 size = uChunkRect.xy + 2.0 * uChunkRect.z;
    // The image's target corner relative to the origin chunk's corner.
    vec2 corner = (uChunk - uOrigin) * uChunkRect.xy - uChunkRect.zz;
    vec2 world = corner + (aPos + 0.5) * size;
    vec2 view = (world - uCamera.xy) * uCamera.zw;
    vec2 screen = uCameraRot.zw + vec2(view.x * uCameraRot.x - view.y * uCameraRot.y, view.x * uCameraRot.y + view.y * uCameraRot.x);
    // World and clip are both y-down, so the mapping carries no flip.
    gl_Position = vec4(screen / uViewport * 2.0 - 1.0, 0.0, 1.0);
    vUv = aPos + 0.5;
  }
`

const TILE_FRAGMENT = glsl`
  in vec2 vUv;
  uniform sampler2D ${CHUNK_SAMPLER};
  // The view's tint over the bake, premultiplied like the bake itself
  // (the layer's tint is baked in); identity [1, 1, 1, 1] until a view
  // sets its own.
  uniform vec4 uTint;
  void main() {
    fragColor = texture(${CHUNK_SAMPLER}, vUv) * uTint;
  }
`

export type TilePipeline = { pipeline: RenderPipelineId; quad: BufferId }

let shared: TilePipeline | undefined

/** The chunk-quad pipeline and its unit quad (a triangle strip of four
 * corners in -0.5..0.5), made once per process. */
export function tilePipeline(): TilePipeline {
  if (shared === undefined) {
    let vs = compileShader("vertex", TILE_VERTEX, { header: true })
    let fs = compileShader("fragment", TILE_FRAGMENT, { header: true })
    let program = linkProgram(vs, fs, { label: "tile-view" })
    destroyShader(vs)
    destroyShader(fs)
    let pipeline = createRenderPipeline(program, {
      label: "tile-view",
      topology: "triangle-strip",
      buffers: [{ attributes: [{ name: "aPos", format: "float32x2" }] }],
      // The bakes composite OVER one another in draw order, the leaf
      // composite's rule: a lower row's overhang over the row above.
      blend: "alpha",
    })
    let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), { label: "tile-view-quad", autoFree: false })
    shared = { pipeline, quad }
  }
  return shared
}

/**
 * The shared params a tile view writes for a camera: the camera point
 * split into its chunk (`uOrigin`) and the remainder (`uCamera.xy`), the
 * zoom in `uCamera.zw`, the rotation and pivot in `uCameraRot` - the
 * sprite stage's cameraParams with the split the stage above needs.
 */
export function tileCameraParams(camera: CameraState, chunkW: number, chunkH: number): ShaderParams {
  let oc = floorDiv(camera.x, chunkW)
  let or = floorDiv(camera.y, chunkH)
  return {
    uOrigin: [oc, or],
    uCamera: [camera.x - oc * chunkW, camera.y - or * chunkH, camera.zoom, camera.zoom],
    uCameraRot: [Math.cos(camera.rotation), Math.sin(camera.rotation), camera.pivotX, camera.pivotY],
  }
}
