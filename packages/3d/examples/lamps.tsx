// Spot and point lights: a dark courtyard under three lamps.
//   - A warm SOFT spot (angle 26, penumbra 0.4) swings from a rig above
//     the crates - ONE setTransform on the parent group per frame, and
//     the pool of light sweeps the floor with a wide fading rim
//     (uLightDir/uLightPos are core-driven, so the moving lamp costs no
//     per-frame JS beyond the rig rotation).
//   - A cool HARD spot (angle 11, penumbra 0.05) stands still, aimed by
//     its `direction` prop at the knot on the pedestal: a crisp disc of
//     light with a sharp rim, for contrast with the soft one.
//   - A blue point bulb orbits between the crates on its own rig: light
//     in every direction, no cone, falling off on the floor around it -
//     and casting: the crates throw blue-edged shadows that wheel
//     around the courtyard as the bulb passes.
// Everything casts: a perspective map of each spot's cone (one shadow
// slot each) and the bulb's six face maps (six slots) - together
// exactly the MAX_SHADOW_MAPS budget of 8, all tiles of the scene's
// one atlas. Faces outside every cone and
// range stay near-black - the hemisphere idles at 0.05.
//
// Intensity under the default decay 2 behaves like candela: the light
// that reaches a face is `intensity / d^2`, so a lamp 5 units up needs
// intensity ~40 for a bright pool, not ~2. `distance` then windows the
// falloff to zero so the pool ends instead of trailing off forever.
import { onFrame, pct, render } from "@solidrt/core"
import { box, cylinder, Group, HemisphereLight, lit, Mesh, PerspectiveCamera, plane, PointLight, Scene, setTransform, SpotLight, torusKnot } from "@solidrt/3d"
import type { SceneNode } from "@solidrt/3d"

function App() {
  let swing!: SceneNode
  let orbit!: SceneNode
  onFrame(tick => {
    // The lamp sways about z so its pool sweeps left-right; the bulb
    // circles the crates on its own axis.
    setTransform(swing, { rotation: [0, 0, Math.sin(tick / 2000) * 0.35] })
    setTransform(orbit, { rotation: [0, tick / 1500, 0] })
  })

  let ground = lit({ color: [0.6, 0.6, 0.62] })
  let crate = lit({ color: [0.7, 0.55, 0.4] })
  let gold = lit({ color: [0.9, 0.75, 0.3], specular: 0.6, shininess: 50 })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.02, 0.02, 0.03, 1]} samples={4} label="lamps">
          <PerspectiveCamera fov={50} position={[0, 5, 8]} lookAt={[0, 0.5, 0]} />
          <HemisphereLight sky={[0.05, 0.05, 0.06]} ground={[0.02, 0.02, 0.02]} />
          <Group ref={g => (swing = g)} position={[0, 5, 0]}>
            <SpotLight
              color={[1, 0.85, 0.6]}
              intensity={40}
              angle={26}
              penumbra={0.4}
              distance={14}
              castShadow
              shadow={{ normalBias: 0.03 }}
            />
          </Group>
          <SpotLight
            position={[-4.5, 5, 1]}
            direction={[1.3, -5, -2.2]}
            color={[0.7, 0.85, 1]}
            intensity={35}
            angle={11}
            penumbra={0.05}
            distance={12}
            castShadow
            shadow={{ mapSize: 512, normalBias: 0.03 }}
          />
          <Group ref={g => (orbit = g)}>
            <PointLight
              position={[2.6, 1, 0]}
              color={[0.4, 0.6, 1]}
              intensity={3}
              distance={8}
              castShadow
              shadow={{ mapSize: 512, normalBias: 0.03 }}
            />
          </Group>
          <Mesh geometry={plane({ width: 20, height: 20 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          <Mesh geometry={box()} material={crate} position={[-1.2, 0.5, 0]} castShadow />
          <Mesh geometry={box()} material={crate} position={[1.6, 0.5, 0.8]} rotation={[0, 0.6, 0]} castShadow />
          <Mesh geometry={box({ width: 0.8, height: 1.6, depth: 0.8 })} material={crate} position={[0.3, 0.8, -1.5]} castShadow />
          <Mesh geometry={cylinder({ radiusTop: 0.35, radiusBottom: 0.45, height: 1 })} material={ground} position={[-3.2, 0.5, -1.2]} castShadow />
          <Mesh geometry={torusKnot({ radius: 0.35, tube: 0.12 })} material={gold} position={[-3.2, 1.5, -1.2]} castShadow />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
