// The landing page's background: the aurora from the intro demo's opening
// screen, one fragment shader on a WebGL2 canvas fixed behind the page. It
// renders at a fixed height and the browser scales it up: the bands are soft,
// so the upscale stays clean and the cost stays flat on large screens. Under
// prefers-reduced-motion it draws one still frame; without WebGL2 the canvas's
// own CSS gradient shows instead. website/index.html carries a copy.

// Render height in pixels; the width follows the viewport's aspect.
const RENDER_HEIGHT = 720;
// The time, in seconds, of the one frame drawn under reduced motion.
const STILL_TIME = 20;

const VERTEX = `#version 300 es
out vec2 vUV;
void main() {
  // One triangle over the whole viewport, placed from the vertex id alone.
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  // Top-left origin, as the SolidRT runtime provides vUV.
  vUV = vec2(p.x, 1.0 - p.y);
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

// The demo's shader, behind the preamble the SolidRT runtime injects
// (vUV, iResolution, fragColor). Soft flowing bands over a deep navy,
// opaque.
const FRAGMENT = `#version 300 es
precision highp float;
in vec2 vUV;
uniform vec2 iResolution;
uniform float uTime;
out vec4 fragColor;

// Animation speed: shader time per second.
const float TIME_SCALE = 0.06;
// fbm: the octave count, the frequency step per octave (off 2.0 so
// octaves do not line up), and the shift between octaves.
const int FBM_OCTAVES = 5;
const float FBM_LACUNARITY = 2.03;
const vec2 FBM_SHIFT = vec2(1.7, 9.2);
// The slow warp field: its scale, and its vertical drift against the
// horizontal one.
const float WARP_SCALE = 1.4;
const float WARP_DRIFT_Y = 0.6;
// The bands: their stretch per axis, how far the warp field bends them,
// and their speed against the warp field's.
const vec2 BAND_SCALE = vec2(0.9, 2.6);
const vec2 BAND_WARP = vec2(1.8, 0.8);
const float BAND_SPEED = 1.6;
// The navy ground, top and bottom, and the band tints.
const vec3 BASE_TOP = vec3(0.020, 0.030, 0.062);
const vec3 BASE_BOTTOM = vec3(0.028, 0.044, 0.100);
const vec3 BLUE = vec3(0.24, 0.40, 0.92);
const vec3 MINT = vec3(0.30, 0.88, 0.80);
const vec3 VIOLET = vec3(0.56, 0.38, 0.98);
// Where a band shows as curtain (the band value range it ramps over),
// the height it fades out over going down, the width it fades in over
// from the left edge, and its brightness over the ground.
const vec2 CURTAIN_BAND = vec2(0.42, 0.92);
const vec2 CURTAIN_FADE_Y = vec2(0.15, 1.05);
const float CURTAIN_FADE_X = 0.35;
const float CURTAIN_STRENGTH = 0.42;
// The tint: warp values blending blue to violet, band values blending
// to mint.
const vec2 VIOLET_RANGE = vec2(0.3, 0.7);
const vec2 MINT_RANGE = vec2(0.62, 0.85);
// A soft glow right of center: its color, center and radius.
const vec3 GLOW_COLOR = vec3(0.05, 0.08, 0.18);
const vec2 GLOW_CENTER = vec2(0.72, 0.42);
const float GLOW_RADIUS = 0.9;
// The vignette: its center, the distance range it darkens over, and how
// much.
const vec2 VIGNETTE_CENTER = vec2(0.55, 0.45);
const vec2 VIGNETTE_RANGE = vec2(0.4, 1.2);
const float VIGNETTE_STRENGTH = 0.35;

// The classic sine hash; its constants are the hash's own, not tuning.
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}
float fbm(vec2 p) {
  float v = 0.0, a = 0.5;
  for (int i = 0; i < FBM_OCTAVES; i++) { v += a * noise(p); p = p * FBM_LACUNARITY + FBM_SHIFT; a *= 0.5; }
  return v;
}
void main() {
  vec2 uv = vUV;
  vec2 p = vec2(uv.x * iResolution.x / iResolution.y, uv.y);
  float t = uTime * TIME_SCALE;
  float n = fbm(p * WARP_SCALE + vec2(t, -t * WARP_DRIFT_Y));
  float bands = fbm(vec2(p.x * BAND_SCALE.x + n * BAND_WARP.x - t * BAND_SPEED, p.y * BAND_SCALE.y + n * BAND_WARP.y));
  vec3 base = mix(BASE_TOP, BASE_BOTTOM, uv.y);
  float curtain = smoothstep(CURTAIN_BAND.x, CURTAIN_BAND.y, bands)
    * (1.0 - smoothstep(CURTAIN_FADE_Y.x, CURTAIN_FADE_Y.y, uv.y))
    * smoothstep(0.0, CURTAIN_FADE_X, uv.x);
  vec3 tint = mix(mix(BLUE, VIOLET, smoothstep(VIOLET_RANGE.x, VIOLET_RANGE.y, n)), MINT, smoothstep(MINT_RANGE.x, MINT_RANGE.y, bands));
  vec3 col = base + curtain * tint * CURTAIN_STRENGTH;
  col += GLOW_COLOR * (1.0 - smoothstep(0.0, GLOW_RADIUS, length(uv - GLOW_CENTER)));
  col *= 1.0 - VIGNETTE_STRENGTH * smoothstep(VIGNETTE_RANGE.x, VIGNETTE_RANGE.y, length(uv - VIGNETTE_CENTER));
  fragColor = vec4(col, 1.0);
}`;

function compile(gl, type, source) {
  let shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    console.warn("Aurora shader failed to compile:", gl.getShaderInfoLog(shader));
    return null;
  }
  return shader;
}

function link(gl) {
  let vertex = compile(gl, gl.VERTEX_SHADER, VERTEX);
  let fragment = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT);
  if (!vertex || !fragment) return null;
  let program = gl.createProgram();
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    console.warn("Aurora shader failed to link:", gl.getProgramInfoLog(program));
    return null;
  }
  return program;
}

function start(canvas, gl) {
  let program = link(gl);
  // An opaque canvas that never draws would cover the page's gradient.
  if (!program) {
    canvas.remove();
    return;
  }
  gl.useProgram(program);
  let uTime = gl.getUniformLocation(program, "uTime");
  let iResolution = gl.getUniformLocation(program, "iResolution");

  let resize = () => {
    let width = Math.round((RENDER_HEIGHT * innerWidth) / innerHeight);
    canvas.width = width;
    canvas.height = RENDER_HEIGHT;
    gl.viewport(0, 0, width, RENDER_HEIGHT);
    gl.uniform2f(iResolution, width, RENDER_HEIGHT);
  };
  let draw = (seconds) => {
    gl.uniform1f(uTime, seconds);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  };
  resize();

  if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
    draw(STILL_TIME);
    addEventListener("resize", () => {
      resize();
      draw(STILL_TIME);
    });
    return;
  }
  // A resize clears the canvas; the next frame repaints it. The browser
  // stops calling frames while the tab is hidden.
  addEventListener("resize", resize);
  let frame = (now) => {
    draw(now / 1000);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

let canvas = document.querySelector("canvas.aurora");
let gl = canvas?.getContext("webgl2", { alpha: false, antialias: false, depth: false });
if (gl) start(canvas, gl);
