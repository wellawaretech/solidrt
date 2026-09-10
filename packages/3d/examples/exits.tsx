// Lifecycle animation on scene nodes, the element tree's vocabulary one
// tree deeper: every crate declares `from` (it pops in from scale zero on
// a bouncy spring) and `exit` (it shrinks away on an ease-in of its own,
// the exit's motion, not the enter's run backwards). Tap a crate: the
// component unmounts and `destroy` lets the node go - it stays drawn
// while it leaves, a ghost no tap or pick reaches, then frees; a fresh
// crate takes the slot a moment later. Tap empty space: the whole shelf
// unmounts through `<Show>`, and the Group's `stagger` spaces the crates'
// exits by their order, then their enters when it comes back. No settle
// callbacks, no closing signals; the scene is static between events.
//
// Debug commands: `state` (the live crate ids), `pixel` ({ index }: the
// WINDOW pixel a crate projects to, for a synthetic tap).
import { createSignal, For, pct, render, Show } from "@solidrt/core"
import { box, Group, Mesh, PerspectiveCamera, plane, Scene, unlit } from "@solidrt/3d"
import type { NodeTransition, SceneHandle, SceneTapEvent, Vec3 } from "@solidrt/3d"
import { registerDebug } from "srt:dev"

const COUNT = 5
// Crate spacing along x, and the pause before a tapped-away crate is
// replaced.
const SPACING = 1.6
const RESPAWN_MS = 700
// Enters bounce in over POP_MS; exits ease in over half of it; the shelf
// cascades its crates STAGGER_MS apart.
const POP_MS = 400
const STAGGER_MS = 70

const CRATE: NodeTransition = {
  scale: {
    duration: POP_MS,
    bounce: 0.45,
    from: [0, 0, 0],
    exit: { value: [0, 0, 0], curve: "ease-in", duration: POP_MS / 2 },
  },
}

// Each crate is keyed by a spawn id, so a respawn is a new node (its own
// enter) and never a write to the old one.
let nextId = 0
let [crates, setCrates] = createSignal<{ id: number; slot: number }[]>(
  Array.from({ length: COUNT }, (_, slot) => ({ id: nextId++, slot })),
)
let [shelf, setShelf] = createSignal(true)
let scene!: SceneHandle
let slotX = (slot: number) => (slot - (COUNT - 1) / 2) * SPACING
let hue = (slot: number): [number, number, number] => [0.4 + 0.5 * (slot / COUNT), 0.35, 0.85 - 0.5 * (slot / COUNT)]

let pop = (id: number, slot: number) => {
  setCrates(list => list.filter(c => c.id !== id))
  setTimeout(() => setCrates(list => [...list, { id: nextId++, slot }]), RESPAWN_MS)
}

function App() {
  registerDebug("state", () => ({ shelf: shelf(), crates: crates() }))
  registerDebug("pixel", (args?: { index?: number }) => {
    let p = scene.project([slotX(args?.index ?? 0), 0.5, 0])
    return p === null ? null : { x: p.x, y: p.y }
  })
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene
          width={800}
          height={500}
          clearColor={[0.07, 0.07, 0.09, 1]}
          label="exits"
          ref={s => (scene = s)}
          onTap={(e: SceneTapEvent) => {
            if (e.mesh === null) setShelf(v => !v)
          }}
        >
          <PerspectiveCamera fov={50} position={[0, 4, 9]} lookAt={[0, 0.5, 0]} />
          <Mesh geometry={plane({ width: 12, height: 6 })} material={unlit({ color: [0.16, 0.17, 0.2] })} rotation={[-Math.PI / 2, 0, 0]} />
          <Show when={shelf()}>
            <Group transition={{ stagger: STAGGER_MS }}>
              <For each={crates()}>
                {c => (
                  <Mesh
                    geometry={box()}
                    material={unlit({ color: hue(c.slot) })}
                    position={[slotX(c.slot), 0.5, 0] as Vec3}
                    transition={CRATE}
                    onTap={() => pop(c.id, c.slot)}
                  />
                )}
              </For>
            </Group>
          </Show>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
