// wave.glsl - animated scanline
//
// A plain fragment body in the injected-preamble dialect: no #version line, so
// createShaderTexture prepends vUV / iResolution / fragColor. The time uniform
// is this file's own declaration, driven by the app through params. Living in
// its own file it stays editable as GLSL instead of as a template literal.
uniform float uTime;
void main() {
  vec2 uv = vUV;
  float wave = sin(uv.x * 12.0 + uTime * 2.0) * 0.06;
  float d = abs(uv.y - 0.5 - wave);
  float line = smoothstep(0.05, 0.0, d);
  vec3 col = mix(vec3(0.05, 0.07, 0.12), vec3(0.2, 0.8, 1.0), line);
  fragColor = vec4(col, 1.0);
}
