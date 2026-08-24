// The sprite pipelines: a unit quad instanced over one atlas, mapped
// world -> clip through uCamera (offset + zoom) and uViewport. World and
// clip space are both y-down (core gpu.ts pixel contract), so the mapping
// carries NO flip anywhere - do not add one. Two record layouts share the
// fragment stage:
// - VERTEX + INSTANCE_ATTRIBUTES: the 13-float interleaved record
//   [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA],
//   used by the records layer (records.ts) and the tile layer (tiles.ts).
// - VERTEX_SPLIT + INSTANCE_ATTRIBUTES_SPLIT: the node-backed live layer
//   (layer.ts), pose and style in separate instance-buffer slots - slot 0
//   is the core-written Pose2D record [x, y, angle, sx, sy], slot 1 the
//   JS-written style record [u0, v0, u1, v1, tintR, tintG, tintB, tintA].
// The rotation here (clockwise, y-down) and pointInSprite in pick.ts must
// agree; the differential check guards pick.ts against an oracle but NOT
// against these shaders - if you touch one rotation, touch all.
import { glsl } from "@solidrt/core/gpu"
import type { InstanceAttribute, VertexAttribute } from "@solidrt/core/gpu"

export let VERTEX = glsl`
  in vec2 aPos;
  in vec2 iCenter;
  in vec2 iSize;
  in vec4 iUv;
  in float iRot;
  in vec4 iTint;
  out vec2 vUv;
  out vec4 vTint;
  uniform vec2 uViewport;
  uniform vec4 uCamera;

  void main() {
    vec2 corner = aPos * iSize;
    float c = cos(iRot), s = sin(iRot);
    vec2 world = iCenter + vec2(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
    vec2 screen = (world - uCamera.xy) * uCamera.zw;
    // World and clip are both y-down, so the mapping carries no flip.
    gl_Position = vec4(screen / uViewport * 2.0 - 1.0, 0.0, 1.0);
    vUv = mix(iUv.xy, iUv.zw, aPos + 0.5);
    vTint = iTint;
  }
`

export let FRAGMENT = glsl`
  in vec2 vUv;
  in vec4 vTint;
  uniform sampler2D uAtlas;

  void main() {
    fragColor = texture(uAtlas, vUv) * vTint;
  }
`

/** The instance attribute list matching the 13-float record layout. */
export const INSTANCE_ATTRIBUTES: VertexAttribute[] = [
  { name: "iCenter", format: "vec2" },
  { name: "iSize", format: "vec2" },
  { name: "iUv", format: "vec4" },
  { name: "iRot", format: "f32" },
  { name: "iTint", format: "vec4" },
]

export let VERTEX_SPLIT = glsl`
  in vec2 aPos;
  in vec2 iPos;
  in float iRot;
  in vec2 iScale;
  in vec4 iUv;
  in vec4 iTint;
  out vec2 vUv;
  out vec4 vTint;
  uniform vec2 uViewport;
  uniform vec4 uCamera;

  void main() {
    vec2 corner = aPos * iScale;
    float c = cos(iRot), s = sin(iRot);
    vec2 world = iPos + vec2(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
    vec2 screen = (world - uCamera.xy) * uCamera.zw;
    // World and clip are both y-down, so the mapping carries no flip.
    gl_Position = vec4(screen / uViewport * 2.0 - 1.0, 0.0, 1.0);
    vUv = mix(iUv.xy, iUv.zw, aPos + 0.5);
    vTint = iTint;
  }
`

/** The split layout: slot 0 the Pose2D record, slot 1 the style record. */
export const INSTANCE_ATTRIBUTES_SPLIT: InstanceAttribute[] = [
  { name: "iPos", format: "vec2" },
  { name: "iRot", format: "f32" },
  { name: "iScale", format: "vec2" },
  { name: "iUv", format: "vec4", slot: 1 },
  { name: "iTint", format: "vec4", slot: 1 },
]
