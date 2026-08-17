// createSound decodes an encoded clip (Ogg/Vorbis or WAV) once, then every
// play() starts a voice with no re-decode - cheap enough for rapid SFX. It is
// reactive-owned: the decoded clip is released and its voices stopped when the
// owning component unmounts, so there is nothing to clean up by hand.
//
// The source is a Uint8Array of encoded bytes. Here they come from a
// `with { type: "binary" }` import (see binary-import.tsx), so the clip is
// baked into the bundle and available synchronously. Bytes loaded at runtime
// (flux:fs, fetch) work the same once you have them.
//
// overlap defaults to true: tapping faster than the clip length stacks voices
// instead of restarting. The second sound sets overlap: false - each play()
// cuts off the previous voice, the single-voice behavior you want for e.g. a
// selection tick. playing() is a signal: true from play() until stop() (it
// does not flip back when a clip ends naturally).
//
// Sounds you can compute need no bytes at all: createPcmSound takes raw
// samples (here a 60 ms sine sweep in an Int16Array) and gives the same handle
// - generate it, do not ship it. Same reactive ownership as createSound.
//
// For a long track (music, ambience) do not load bytes at all: pass a file
// path to createSoundStream from the same module, which decodes from disk on
// demand and stays off the heap. Same play()/stop()/playing() surface,
// always single-voice.
import { render } from "@solidrt/core"
import { createPcmSound, createSound } from "@solidrt/core/sound"
import blipBytes from "./blip.wav" with { type: "binary" }

const RATE = 44100
// A short sweep from 880 Hz down to 440 Hz with a linear fade-out.
function sweep(): Int16Array {
  let n = Math.round(RATE * 0.06)
  let out = new Int16Array(n)
  let phase = 0
  for (let i = 0; i < n; i++) {
    let t = i / n
    phase += (2 * Math.PI * (880 - 440 * t)) / RATE
    out[i] = Math.round(Math.sin(phase) * (1 - t) * 0.6 * 32767)
  }
  return out
}

function Button(props: { label: string; onTap: () => void }) {
  return (
    <view onPointerDown={props.onTap} padding={12} clipRadius={8}>
      <d-rect color="#333" />
      <text color="#e6e6e6">{props.label}</text>
    </view>
  )
}

function App() {
  let blip = createSound(blipBytes, { gain: 0.8 })
  let tick = createSound(blipBytes, { overlap: false })
  let synth = createPcmSound(sweep(), RATE)

  return (
    <window padding={20} gap={8} alignItems="flex-start">
      <Button label="Blip (tap fast to stack voices)" onTap={() => blip.play()} />
      <Button label="Tick (overlap: false, restarts)" onTap={() => tick.play()} />
      <Button label="Sweep (createPcmSound, generated)" onTap={() => synth.play()} />
      <Button label="Stop" onTap={() => { blip.stop(); tick.stop(); synth.stop() }} />
      <text color="#888">{blip.playing() || tick.playing() || synth.playing() ? "playing" : "silent"}</text>
    </window>
  )
}

render(() => <App />)
