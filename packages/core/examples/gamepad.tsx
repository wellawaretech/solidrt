// Gamepad test bed: every connected pad's raw snapshot (gamepads()) at the
// top, and two players seated by gamepad.next() below - press any button
// on a pad and it joins player 1, another pad joins player 2. Each player
// is an input map (`move` vec2 on the left stick and the dpad, `zoom` axis
// on the triggers, `jump` button on south) whose values print live, plus
// a marker that walks with `move`, so what a pad does is on screen.
// Nothing moves until a pad does.
import { createInputMap, createSignal, gamepad, gamepads, onFrame, render, untrack, windowSize, For } from "@solidrt/core"
import type { Vec2 } from "@solidrt/core"

const PLAYERS = 2
// Marker travel at full stick deflection, pixels per second.
const SPEED = 300
const MARKER = 28
const COLORS = ["#ffcc4d", "#66e6ff"]

let fmt = (v: number) => (v < 0 ? "" : " ") + v.toFixed(2)
let clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

function App() {
  let players = Array.from({ length: PLAYERS }, (_, i) => {
    let pad = gamepad.next()
    let input = createInputMap({ move: "vec2", zoom: "axis", jump: "button" })
    input.bind("move", pad.leftStick, pad.dpad)
    input.bind("zoom", pad.triggers)
    input.bind("jump", pad.button("south"))
    // The marker: plain numbers stepped per frame, a signal to redraw.
    let x = 80 + i * 120
    let y = 260
    let [pos, setPos] = createSignal<Vec2>([x, y])
    let [jumps, setJumps] = createSignal(0)
    input.onPress("jump", () => setJumps(n => n + 1))
    let step = (dt: number) => {
      let [mx, my] = untrack(() => input.value("move"))
      if (mx === 0 && my === 0) return
      let size = untrack(windowSize)
      x = clamp(x + mx * SPEED * dt, 0, Math.max(0, size.width - MARKER))
      y = clamp(y + my * SPEED * dt, 0, Math.max(0, size.height - MARKER))
      setPos([x, y])
    }
    return { pad, input, pos, jumps, step }
  })
  let last: number | null = null
  onFrame(tick => {
    let dt = last === null ? 0 : (tick - last) / 1000
    last = tick
    for (let p of players) p.step(dt)
  })
  let padLine = (slot: number, pad: { name: string; buttons: string[]; axes: Record<string, number> } | null) => {
    if (!pad) return `slot ${slot}: empty`
    let a = pad.axes
    return `slot ${slot}: ${pad.name} - buttons [${pad.buttons.join(" ")}] left${fmt(a.leftX ?? 0)},${fmt(a.leftY ?? 0)} right${fmt(a.rightX ?? 0)},${fmt(a.rightY ?? 0)} triggers${fmt(a.leftTrigger ?? 0)},${fmt(a.rightTrigger ?? 0)}`
  }
  return (
    <window>
      <d-rect color="#181820" />
      <view flexDirection="column" gap={10} padding={24}>
        <text color="#eef4ff" fontSize={22} fontWeight={700}>
          Gamepads
        </text>
        <text color="#a9bcd6" fontSize={14}>
          press any button on a pad to join as player 1, another pad for player 2 - left stick or dpad walks the marker, triggers read zoom, south counts jumps
        </text>
        <view flexDirection="column" gap={2}>
          <text color="#a9bcd6" fontSize={14}>
            connected: {String(gamepads().filter(p => p !== null).length)}
          </text>
          <For each={gamepads()} keyed={false}>
            {(pad, i) => (
              <text color="#eef4ff" fontSize={14}>
                {padLine(i, pad())}
              </text>
            )}
          </For>
        </view>
        <For each={players}>
          {(p, i) => (
            <view flexDirection="column" gap={2}>
              <text color={COLORS[i()]} fontSize={16} fontWeight={700}>
                player {String(i() + 1)}: {p.pad.slot === undefined ? "press any button on a pad to join" : `pad ${p.pad.slot}`}
              </text>
              <text color="#eef4ff" fontSize={14}>
                move{fmt(p.input.value("move")[0])},{fmt(p.input.value("move")[1])} zoom{fmt(p.input.value("zoom"))} jump {p.input.pressed("jump") ? "down" : "up"} (x{String(p.jumps())})
              </text>
            </view>
          )}
        </For>
      </view>
      <For each={players}>{(p, i) => <d-rect x={p.pos()[0]} y={p.pos()[1]} w={MARKER} h={MARKER} radius={6} color={COLORS[i()]} />}</For>
    </window>
  )
}

render(() => <App />)
