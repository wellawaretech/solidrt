// record() of flux:video over the test camera and microphone: a clip of
// the installed image and the installed sound lands in a WebM file the
// player opens with the camera's size and an audio track; a recording
// pauses, a missing source rejects, and a bad argument throws. The
// sessions are opened inside mount through core's wrappers, as the camera
// and microphone tests do, and handed to record() by texture id and
// session id. A node source records the window (or a node's box cut out
// of it) through the window tap, at the size asked.

import { test, expect } from "@solidrt/test"
import { createCamera } from "@solidrt/core/camera"
import type { CameraStream } from "@solidrt/core/camera"
import { createMicrophone } from "@solidrt/core/microphone"
import type { MicrophoneStream } from "@solidrt/core/microphone"
import { createVideo } from "@solidrt/core/video"
import type { VideoStream } from "@solidrt/core/video"
import type { TextureId } from "@solidrt/core/gpu"
import { record } from "flux:video"
import { file } from "flux:fs"
import { Endpoint } from "flux:p2p"
import type { P2pStream } from "flux:p2p"
import { connect, listen } from "flux:net"
import type { Conn } from "flux:net"
import { createEffect, createRoot, createSignal, Show } from "solid-js"

const WIDTH = 64
const HEIGHT = 48
const RATE = 16000
// One second of a 440 Hz tone.
const TONE_SECONDS = 1
const TONE_HZ = 440
// The still camera publishes one frame per install: each install is one
// frame of the clip.
const STILL_FRAMES = 5
// Frames to step while the recording takes the tone and the stills.
const RUN_FRAMES = 30

function still(shade: number): { width: number; height: number; data: Uint8Array } {
  let data = new Uint8Array(WIDTH * HEIGHT * 4)
  for (let i = 0; i < WIDTH * HEIGHT; i++) data.set([shade, 64, 255 - shade, 255], i * 4)
  return { width: WIDTH, height: HEIGHT, data }
}

function tone(): Float32Array {
  let samples = new Float32Array(RATE * TONE_SECONDS)
  for (let i = 0; i < samples.length; i++) samples[i] = 0.5 * Math.sin((2 * Math.PI * TONE_HZ * i) / RATE)
  return samples
}

test("a recording of the camera and the microphone plays back", async app => {
  // Installed before mount, so the app's opens find the test devices; the
  // recording hears what is installed after it starts.
  await app.camera(still(0))
  await app.microphone({ sampleRate: RATE, samples: new Float32Array(0) })
  let cam!: CameraStream
  let mic!: MicrophoneStream
  // The playback of the clip, shown once it exists (a test is one app, so
  // the player mounts under a switch rather than in a second mount).
  let [showClip, setShowClip] = createSignal(false)
  let clip: VideoStream | undefined
  function Playback() {
    clip = createVideo(file("clip.webm"))
    return <view />
  }
  await app.mount(() => {
    cam = createCamera()
    mic = createMicrophone({ sampleRate: RATE })
    return (
      <view>
        <Show when={showClip()}>
          <Playback />
        </Show>
      </view>
    )
  })
  await app.settle()
  expect(cam.error()).toBe(undefined)
  expect(mic.error()).toBe(undefined)
  let rec = await record({ video: { texture: cam.texture()! }, audio: { id: mic.id()! } }, { to: file("clip.webm") })
  expect(rec.recording()).toBe(true)
  expect(rec.error()).toBe(undefined)
  // The sound arrives once the recording listens; each new still is a frame.
  await app.microphone({ sampleRate: RATE, samples: tone() })
  for (let i = 1; i <= STILL_FRAMES; i++) await app.camera(still(i * 40))
  for (let i = 0; i < RUN_FRAMES; i++) await app.frame()
  await app.settle()
  await rec.stop()
  expect(rec.recording()).toBe(false)
  expect(rec.error()).toBe(undefined)
  expect(rec.duration()).toBeGreaterThan(0)

  setShowClip(true)
  await app.settle()
  expect(clip?.error()).toBe(undefined)
  expect([clip?.width(), clip?.height()]).toEqual([WIDTH, HEIGHT])
  expect(clip?.duration()).toBeGreaterThanOrEqual(TONE_SECONDS - 0.1)
})

test("a paused recording takes nothing and a second stop is a no-op", async app => {
  await app.camera(still(0))
  let cam!: CameraStream
  await app.mount(() => {
    cam = createCamera()
    return <view />
  })
  await app.settle()
  let rec = await record({ video: { texture: cam.texture()! } }, { to: file("paused.webm"), frameRate: 10 })
  rec.pause()
  expect(rec.recording()).toBe(false)
  rec.resume()
  expect(rec.recording()).toBe(true)
  await rec.stop()
  await rec.stop()
  expect(rec.recording()).toBe(false)
})

// The window the screen tests mount, and the size the screencast is asked
// at: one dimension, the other follows the window's aspect.
const SCREEN_HEIGHT = 48

// A screen test's app: a colored box at `left` in the window (moved every
// frame, so each frame draws: a window at rest records nothing), and the
// clip's player mounted under a switch once the recording is done.
const BOX_WIDTH = 40
const BOX_HEIGHT = 30
const BOX_STEP = 2

function screenApp(
  box: (node: { id: number }) => void,
  left: () => number,
  name: string,
  show: () => boolean,
  got: (clip: VideoStream) => void,
) {
  function Playback() {
    got(createVideo(file(name)))
    return <view />
  }
  return (
    <view flex={1}>
      <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#2060a0" />
      <view ref={box} position="absolute" left={left()} top={10} width={BOX_WIDTH} height={BOX_HEIGHT}>
        <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#a02020" />
      </view>
      <Show when={show()}>
        <Playback />
      </Show>
    </view>
  )
}

test("a recording of the window lands at the size asked", async app => {
  let [showClip, setShowClip] = createSignal(false)
  let [left, setLeft] = createSignal(0)
  let clip: VideoStream | undefined
  let win = await app.mount(() => screenApp(() => {}, left, "screen.webm", showClip, c => (clip = c)))
  await app.settle()
  let { width, height } = win.record
  let rec = await record({ video: { node: win.record.id } }, { to: file("screen.webm"), height: SCREEN_HEIGHT })
  expect(rec.recording()).toBe(true)
  // Every frame moves the box, so every stepped frame draws and is read
  // back; a readback lands at the next frame, the last one at the stop.
  for (let i = 0; i < RUN_FRAMES; i++) {
    setLeft(left() + BOX_STEP)
    await app.frame()
  }
  await app.settle()
  await rec.stop()
  expect(rec.error()).toBe(undefined)
  expect(rec.duration()).toBeGreaterThan(0)
  setShowClip(true)
  await app.settle()
  expect(clip?.error()).toBe(undefined)
  let even = (v: number) => Math.ceil(v / 2) * 2
  expect(clip?.height()).toBe(SCREEN_HEIGHT)
  expect(clip?.width()).toBe(even(Math.floor((SCREEN_HEIGHT * width) / height)))
})

test("a node records its box cut out of the window", async app => {
  let [showClip, setShowClip] = createSignal(false)
  let [left, setLeft] = createSignal(0)
  let clip: VideoStream | undefined
  let box = app.ref()
  await app.mount(() => screenApp(box, left, "box.webm", showClip, c => (clip = c)))
  await app.settle()
  let rec = await record({ video: { node: box.record.id } }, { to: file("box.webm") })
  // The cut-out follows the box as it moves.
  for (let i = 0; i < RUN_FRAMES; i++) {
    setLeft(left() + BOX_STEP)
    await app.frame()
  }
  await app.settle()
  await rec.stop()
  expect(rec.error()).toBe(undefined)
  setShowClip(true)
  await app.settle()
  expect(clip?.error()).toBe(undefined)
  // The box's own pixel size (the test display is at scale 1).
  expect([clip?.width(), clip?.height()]).toEqual([BOX_WIDTH, BOX_HEIGHT])
})

// Frames stepped while the live loopback runs: a few seconds of app time,
// past the live resume buffer, so the player shows frames.
const LIVE_FRAMES = 120
const LIVE_LATENCY_S = 0.3
// The player's open across the stream, as a promise: resolved by its
// outcome, open or failed. A live player is never at rest, so settle is no
// way to wait for it; and stepping frames is no way either, since the open
// is work in flight on the native side that progresses while the test
// awaits (the hold it takes keeps the engine polling), not per frame. A
// test whose open never lands fails on its own cap, naming the open.
function opened(clip: VideoStream): Promise<void> {
  return new Promise(resolve =>
    createRoot(dispose =>
      createEffect(
        () => clip.ready() || clip.error() !== undefined,
        done => {
          if (done) {
            resolve()
            queueMicrotask(dispose)
          }
        },
      ),
    ),
  )
}

test("a live recording of the window plays back through a stream", async app => {
  let [showPlayer, setShowPlayer] = createSignal(false)
  let [left, setLeft] = createSignal(0)
  let clip: VideoStream | undefined
  let readable: ReadableStream<Uint8Array> | undefined
  // The player over the recording's own stream: a loopback on one client.
  function Live() {
    clip = createVideo(readable!, { latency: LIVE_LATENCY_S, autoplay: true })
    return <view />
  }
  let win = await app.mount(() => (
    <view flex={1}>
      <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#2060a0" />
      <view position="absolute" left={left()} top={10} width={BOX_WIDTH} height={BOX_HEIGHT}>
        <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#a02020" />
      </view>
      <Show when={showPlayer()}>
        <Live />
      </Show>
    </view>
  ))
  await app.settle()
  let rec = await record({ video: { node: win.record.id } }, { height: SCREEN_HEIGHT })
  expect(rec.readable).toBeInstanceOf(ReadableStream)
  readable = rec.readable
  setShowPlayer(true)
  for (let i = 0; i < LIVE_FRAMES; i++) {
    setLeft(left() + BOX_STEP)
    await app.frame()
  }
  await opened(clip!)
  expect(clip?.error()).toBe(undefined)
  expect(clip?.height()).toBe(SCREEN_HEIGHT)
  expect(clip?.duration()).toBe(undefined)
  expect(rec.error()).toBe(undefined)
  await rec.stop()
  expect(rec.recording()).toBe(false)
})

// A live recording across a real p2p stream, both ends in this one app:
// the shower accepts, the caster connects by ticket and pipes the
// recording into the stream, the shower plays the stream.
const P2P_PROTOCOL = "record-test"

test("a live recording crosses a p2p stream into a player", async app => {
  let [showPlayer, setShowPlayer] = createSignal(false)
  let [left, setLeft] = createSignal(0)
  let clip: VideoStream | undefined
  let incoming: P2pStream | undefined
  function Live() {
    clip = createVideo(incoming!.readable, { latency: LIVE_LATENCY_S, autoplay: true })
    return <view />
  }
  let win = await app.mount(() => (
    <view flex={1}>
      <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#2060a0" />
      <view position="absolute" left={left()} top={10} width={BOX_WIDTH} height={BOX_HEIGHT}>
        <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#a02020" />
      </view>
      <Show when={showPlayer()}>
        <Live />
      </Show>
    </view>
  ))
  await app.settle()
  let shower = await Endpoint.create({ protocols: [P2P_PROTOCOL] })
  let caster = await Endpoint.create()
  let accepted = (async () => {
    for await (let stream of shower.accept(P2P_PROTOCOL)) return stream
    return undefined
  })()
  let outgoing = await caster.connect(await shower.ticket(), P2P_PROTOCOL)
  let rec = await record({ video: { node: win.record.id } }, { height: SCREEN_HEIGHT })
  let piped = rec.readable!.pipeTo(outgoing.writable)
  incoming = await accepted
  expect(incoming).toBeDefined()
  setShowPlayer(true)
  for (let i = 0; i < LIVE_FRAMES; i++) {
    setLeft(left() + BOX_STEP)
    await app.frame()
  }
  await opened(clip!)
  expect(clip?.error()).toBe(undefined)
  expect(clip?.height()).toBe(SCREEN_HEIGHT)
  await rec.stop()
  await piped.catch(() => {})
  incoming?.close()
  outgoing.close()
  await caster.close()
  await shower.close()
})

// The same over a TCP connection on the loopback (flux:net), left running
// at the end of the test: the engine's teardown must end the recording,
// the pipe and the connection without waiting on any of them.
test("a live recording crosses a TCP connection, and teardown ends it", async app => {
  let [showPlayer, setShowPlayer] = createSignal(false)
  let [left, setLeft] = createSignal(0)
  let clip: VideoStream | undefined
  let incoming: Conn | undefined
  function Live() {
    clip = createVideo(incoming!.readable, { latency: LIVE_LATENCY_S, autoplay: true })
    return <view />
  }
  let win = await app.mount(() => (
    <view flex={1}>
      <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#2060a0" />
      <view position="absolute" left={left()} top={10} width={BOX_WIDTH} height={BOX_HEIGHT}>
        <rect position="absolute" top={0} left={0} right={0} bottom={0} color="#a02020" />
      </view>
      <Show when={showPlayer()}>
        <Live />
      </Show>
    </view>
  ))
  await app.settle()
  let listener = await listen(0)
  let port = Number(listener.localAddr.slice(listener.localAddr.lastIndexOf(":") + 1))
  let accepted = (async () => {
    for await (let conn of listener) return conn
    return undefined
  })()
  let outgoing = await connect("127.0.0.1", port)
  let rec = await record({ video: { node: win.record.id } }, { height: SCREEN_HEIGHT })
  void rec.readable!.pipeTo(outgoing.writable).catch(() => {})
  incoming = await accepted
  expect(incoming).toBeDefined()
  setShowPlayer(true)
  for (let i = 0; i < LIVE_FRAMES; i++) {
    setLeft(left() + BOX_STEP)
    await app.frame()
  }
  await opened(clip!)
  expect(clip?.error()).toBe(undefined)
  expect(clip?.height()).toBe(SCREEN_HEIGHT)
  // Everything is left open on purpose: the recording, the pipe, both
  // ends of the connection and the listener end with the engine.
})

test("a source that is not open rejects, and a bad argument throws", async app => {
  await app.mount(() => <view />)
  await expect(record({ video: { texture: 999999 as TextureId } }, { to: file("none.webm") })).rejects.toMatchObject({ kind: "unsupported" })
  await expect(record({ audio: { id: 999999 } }, { to: file("none.webm") })).rejects.toMatchObject({ kind: "unsupported" })
  await expect(record({ video: { node: 999999 } }, { to: file("none.webm") })).rejects.toMatchObject({ kind: "unsupported" })
  expect(() => record({}, { to: file("none.webm") })).toThrow()
  expect(() => record({ video: { texture: 1 as TextureId } }, { to: 5 as never })).toThrow()
  expect(() => record({ video: { node: 1 } }, { to: file("none.webm"), height: 0 })).toThrow()
})
