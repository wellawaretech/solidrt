// The `output` prop: Scene hands you its texture id and you compose the
// leaf yourself, in place of the built-in `<texture>`. Here a fragment
// pass samples the scene and adds chromatic aberration plus a vignette,
// and the scene renders at 2x display size - the post pass doubles as the
// downsample (the default linear sampler box-averages the 2x2 quad), so
// the same chain is also free supersampling. Sampled textures are live
// dependencies: the post pass re-renders exactly when the scene target
// does, and a static scene still costs zero passes.
//
// The same slot takes a `<d-texture>`, a leaf with blendMode/fit/pointer
// props, or `() => null` (headless - composite scene.texture elsewhere).
import { createSignal, onFrame, pct, render } from "@solidrt/core"
import { createShaderTexture } from "@solidrt/core/gpu"
import { box, Group, Mesh, PerspectiveCamera, plane, Scene, sphere, unlit } from "@solidrt/3d"

const SIZE = 720 // display pixels (the leaf)
const RENDER = SIZE * 2 // target pixels (the scene)

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
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Scene
          width={RENDER}
          height={RENDER}
          clearColor={[0.07, 0.07, 0.1, 1]}
          label="scene-post"
          output={tex => {
            // Created inside the callback, the post target disposes with
            // the Scene.
            let post = createShaderTexture(POST, SIZE, SIZE, null, { textures: { uSource: tex } })
            return <texture src={post} width={SIZE} height={SIZE} />
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
