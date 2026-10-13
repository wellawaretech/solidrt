// The `output` prop on a fill scene: Scene hands you its texture id and
// the leaf contract, and you compose the leaf yourself in place of the
// built-in `<texture>`. Here a fragment pass samples the scene and adds
// chromatic aberration plus a vignette. The scene fills its pane at
// `renderScale={2}` (four times the device pixels) and the post target is
// sized at display density from `leaf.deviceSize()`, so the post pass
// doubles as the downsample (the linear sampler box-averages the 2x2
// quad): the same chain is free supersampling. `ref={leaf.ref}` is what
// lets the target follow the leaf, and `leaf.handlers` would route pointer
// events through it. Sampled textures are live dependencies: the post pass
// re-renders exactly when the scene target does, and a static scene still
// costs zero passes; createShaderTextureMemo resizes the post target in
// place when the pane or the display scale changes.
//
// The same slot takes a `<d-texture>`, a leaf with blendMode/fit/pointer
// props, or `() => null` (headless - composite scene.texture elsewhere).
// A `maxPixels` next to `renderScale` would hold the scene's cost on any
// display; scene-views.tsx shows the fixed-size form.
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createShaderTextureMemo } from "@solidrt/core/gpu"
import { box, Group, Mesh, PerspectiveCamera, plane, Scene, sphere, unlit } from "@solidrt/3d"

// Device pixels the scene renders per on-screen pixel, each axis.
const RENDER_SCALE = 2

// vUV, iResolution, and fragColor come from the standard shader-texture
// preamble; uSource is bound via the `textures` option.
const POST = `
  uniform sampler2D uSource;
  void main() {
    vec2 c = vUV - 0.5;
    float r2 = dot(c, c);
    // Chromatic aberration: red and blue sample at slightly shifted radii.
    vec2 shift = c * r2 * 0.04;
    vec3 col = vec3(
      texture(uSource, vUV + shift).r,
      texture(uSource, vUV).g,
      texture(uSource, vUV - shift).b);
    // Vignette.
    col *= 1.0 - 1.1 * r2;
    fragColor = vec4(col, 1.0);
  }`

function App() {
  let [spin, setSpin] = createSignal(0)
  onFrame(tick => setSpin(tick / 2000))

  let cube = box()
  let floor = plane({ width: 6, height: 6, label: "floor" })
  let ball = sphere({ radius: 0.35 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene
          renderScale={RENDER_SCALE}
          clearColor={[0.07, 0.07, 0.1, 1]}
          label="scene-post"
          output={(tex, leaf) => {
            // Created inside the callback, the post target disposes with
            // the Scene; its size follows the leaf's on-screen size.
            let post = createShaderTextureMemo(() => ({
              fragmentSrc: POST,
              width: leaf.deviceSize().width,
              height: leaf.deviceSize().height,
              textures: { uSource: tex },
            }))
            return <texture ref={leaf.ref} src={post()} width={pct(100)} height={pct(100)} contain="size" />
          }}
        >
          <PerspectiveCamera fov={55} position={[0, 1.6, 3.6]} lookAt={[0, 0.3, 0]} />
          <Mesh
            geometry={floor}
            material={unlit({ color: [0.16, 0.17, 0.22] })}
            rotation={[-Math.PI / 2, 0, 0]}
          />
          <Group rotation={[0, spin(), 0]}>
            <Mesh geometry={cube} material={unlit({ color: [0.85, 0.3, 0.3] })} position={[0, 0.5, 0]} />
            <Mesh
              geometry={cube}
              material={unlit({ color: [0.9, 0.8, 0.35] })}
              position={[-1.1, 0.7, 0]}
              scale={[0.5, 1.4, 0.5]}
            />
            <Mesh geometry={ball} material={unlit({ color: [0.35, 0.65, 0.9] })} position={[1.1, 0.35, 0]} />
          </Group>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
