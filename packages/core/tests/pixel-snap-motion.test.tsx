// Pixel snapping and motion (okf/done/pixel-snap-at-draw.md). A box, a
// text and a movement are rounded to whole device pixels where they are
// drawn, once. A running transition that moves a box turns the rounding
// off for its subtree until it settles, and `pixelSnap={false}` turns it
// off for a subtree for good. What only the picture shows is read here,
// frame by frame, as each frame's own paint drew it:
//
// - a letter of an animated title (a d-view with springs on y, scale and
//   rotate around a d-text, every origin on a half pixel) ends its motion
//   with no step under `pixelSnap={false}`, and without it stays unrounded
//   while its tracks run and is rounded only when the last ends;
// - nothing rasterizes again when a scale that came down rests;
// - a number that pops in from larger moves one way only, as text and as
//   a snapshot boundary;
// - a box under a held translate stays on the grid, leaves it while the
//   track runs and is back on it when the track ends.

import { test, expect } from "@solidrt/test"
import type { Pixels, TestApp } from "@solidrt/test"
import { createSignal, Show } from "@solidrt/core"

type Ink = { cx: number; cy: number; sum: number }

/** Where the ink is in columns [from, to): its centre, x measured from `origin`. */
function ink(pixels: Pixels, from: number, to: number, origin = from): Ink {
  let { width, height, data } = pixels
  let sum = 0
  let sx = 0
  let sy = 0
  for (let y = 0; y < height; y++) {
    for (let x = from; x < to; x++) {
      let a = data[(y * width + x) * 4 + 3]!
      sum += a
      sx += a * (x - origin)
      sy += a * y
    }
  }
  return { cx: sx / sum, cy: sy / sum, sum }
}

/** How far the ink moved between two reads, the larger axis, in device px. */
function moved(a: Ink, b: Ink): number {
  return Math.max(Math.abs(a.cx - b.cx), Math.abs(a.cy - b.cy))
}

/**
 * The most the frame a track settles on may move the ink: its `roundings`
 * (the box's own, and one more for each translate above it, which rounds
 * as a movement), on top of the motion's own last step, which is no larger
 * than the step of the frame before it.
 */
function settleBound(reads: Ink[], settle: number, roundings: number): number {
  return roundings * ROUNDING_PX + moved(reads[settle - 2]!, reads[settle - 1]!) + ALIKE_PX
}

/** Whether rows [from, to) hold a pixel that is neither empty nor fully covered. */
function partial(pixels: Pixels, from: number, to: number): boolean {
  let { width, data } = pixels
  for (let i = from * width; i < to * width; i++) {
    let a = data[i * 4 + 3]!
    if (a !== 0 && a !== 255) return true
  }
  return false
}

/** The text layers the last frame rasterized. */
function textLayers(app: TestApp): number {
  return (app.stats({ windowFrames: 1 }) as { paintOps: { textLayers: number } }).paintOps.textLayers
}

// A rounding moves a box by at most half a device pixel per axis.
const ROUNDING_PX = 0.5
// Two reads of one picture agree to this: what the float sums leave.
const ALIKE_PX = 0.005

// ---- The title letter ----

// A title on the fractions that make a rounding show: display scale 1, a
// design-size fit of 1.5 centred in a window one pixel wider than it (so
// the fitted view starts on a half pixel), the title on a half pixel under
// it, and each letter translated to its centre (70.5 device px down) with
// its text offset back. The letter's x puts its resting place a clear
// fraction of a pixel off the grid, so the one rounding at rest shows. Two
// titles, a whole number of device pixels apart, so the
// same letter is drawn twice at the same fractions: one rounded as any
// element is, one under pixelSnap={false}.
const WINDOW: [number, number] = [421, 281]
const DESIGN: [number, number] = [280, 140]
const FIT = 1.5
const TITLE = { top: 20, width: 90, height: 94 }
const SNAPPED_LEFT = 10
const FREE_LEFT = 110
const LETTER = { x: 20.2, w: 46 }
const TITLE_PX = 80
const TILT = -0.12
const POSES = {
  rest: { y: 0, scale: 1, lean: 0 },
  hover: { y: -4, scale: 1.1, lean: 0.5 },
}
type Pose = keyof typeof POSES
// What rounds a letter at rest: its translate, and its text.
const LETTER_ROUNDINGS = 2
// The device column between the two titles, and how far apart they are.
const SPLIT_PX = Math.round(((SNAPPED_LEFT + TITLE.width + FREE_LEFT) / 2) * FIT)
const APART_PX = (FREE_LEFT - SNAPPED_LEFT) * FIT
// The hover holds this long, the release is read for this long (past the
// longest spring), and from TAIL_FROM on the springs move the letter by
// less than SMOOTH_PX a frame: a larger move there is a step.
const HOVER_FRAMES = 40
const RELEASE_FRAMES = 70
const TAIL_FROM = 20
const SMOOTH_PX = 0.05
// The raster a release makes for the scale it ends at falls in its first
// frames, at the turn, where the motion hides it.
const TURN_FRAMES = 5

function Title(props: { left: number; pose: Pose; pixelSnap?: boolean }) {
  let pose = () => POSES[props.pose]
  return (
    <view pixelSnap={props.pixelSnap} position="absolute" left={props.left} top={TITLE.top} width={TITLE.width} height={TITLE.height}>
      <d-view
        x={LETTER.x + LETTER.w / 2}
        y={TITLE.height / 2 + pose().y}
        scale={pose().scale}
        rotate={TILT * pose().lean}
        transition={{
          y: { duration: 400, bounce: 0.4 },
          scale: { duration: 400, bounce: 0.5 },
          rotate: { duration: 520, bounce: 0.4 },
        }}
      >
        <d-text x={-LETTER.w / 2} y={-TITLE.height / 2} w={LETTER.w} color="#ffffff" fontSize={TITLE_PX}>
          n
        </d-text>
      </d-view>
    </view>
  )
}

async function mountTitles(app: TestApp) {
  let [pose, setPose] = createSignal<Pose>("rest")
  let stage = app.ref()
  await app.mount(() => (
    <view ref={stage} flex={1} alignItems="center" justifyContent="center">
      <view width={DESIGN[0] * FIT} height={DESIGN[1] * FIT} designSize={DESIGN}>
        <Title left={SNAPPED_LEFT} pose={pose()} />
        <Title left={FREE_LEFT} pose={pose()} pixelSnap={false} />
      </view>
    </view>
  ))
  await app.settle()
  return { stage, setPose }
}

test(
  "a letter ends its motion with no step under pixelSnap={false}, and with one step without it",
  async app => {
    let { stage, setPose } = await mountTitles(app)
    setPose("hover")
    await app.frame(HOVER_FRAMES)
    setPose("rest")
    let snapped: Ink[] = []
    let free: Ink[] = []
    for (let f = 0; f < RELEASE_FRAMES; f++) {
      let frame = await app.painted(stage)
      snapped.push(ink(frame, 0, SPLIT_PX))
      free.push(ink(frame, SPLIT_PX, frame.width, APART_PX))
    }
    // Under pixelSnap={false} the letter ends where its motion put it: no
    // frame of the tail moves it by more than the springs do, and it is at
    // rest at the end.
    for (let f = TAIL_FROM; f < RELEASE_FRAMES; f++) expect(moved(free[f - 1]!, free[f]!)).toBeLessThan(SMOOTH_PX)
    expect(moved(free[RELEASE_FRAMES - 2]!, free[RELEASE_FRAMES - 1]!)).toBe(0)
    // Without it the letter is rounded when its last track settles, and
    // only then: one frame of the tail steps, by no more than the letter's
    // two roundings (its translate, as a movement, and its text).
    let steps: number[] = []
    for (let f = TAIL_FROM; f < RELEASE_FRAMES; f++) if (moved(snapped[f - 1]!, snapped[f]!) >= SMOOTH_PX) steps.push(f)
    expect(steps.length).toBe(1)
    let settle = steps[0]!
    expect(moved(snapped[settle - 1]!, snapped[settle]!)).toBeLessThanOrEqual(settleBound(snapped, settle, LETTER_ROUNDINGS))
    // Until then nothing rounds it: while any of its tracks runs it is the
    // same picture as the unrounded letter, whichever of them has settled.
    for (let f = 0; f < settle; f++) expect(moved(snapped[f]!, free[f]!)).toBeLessThan(ALIKE_PX)
    expect(moved(snapped[RELEASE_FRAMES - 2]!, snapped[RELEASE_FRAMES - 1]!)).toBe(0)
  },
  { size: WINDOW, scale: 1 },
)

test(
  "a text under a scale that came down does not rasterize again when the scale rests",
  async app => {
    let { setPose } = await mountTitles(app)
    setPose("hover")
    await app.frame(HOVER_FRAMES)
    setPose("rest")
    // No read of the picture here: a read is a second look at the text in
    // its frame, and the text counts consecutive looks to know it rests.
    let late = 0
    for (let f = 0; f < RELEASE_FRAMES; f++) {
      await app.frame()
      if (f >= TURN_FRAMES) late += textLayers(app)
    }
    expect(late).toBe(0)
  },
  { size: WINDOW, scale: 1 },
)

// ---- The pop ----

// A number that pops in from slightly larger, about the centre of its box,
// at display scale 2: the text at the box's top left, so the shrinking
// scale carries its ink right and down, by several pixels over the pop.
// Beside it the same under a snapshot boundary.
const POP = { duration: 180, curve: "ease-out", from: 1.18 } as const
const POP_WINDOW: [number, number] = [340, 140]
const POP_SCALE = 2
const POP_BOX = { width: 140, height: 90 }
const POP_AT = { left: 20.3, top: 17.7 }
const NUMBER_PX = 44
// The device column between the two numbers: past the first one's ink.
const POP_SPLIT_PX = 135 * POP_SCALE
// Frames read: the pop's 180 ms and a few at rest.
const POP_FRAMES = 16
// The reads the mount's own rasters show in: the frame that mounts the
// numbers and the one after, where its count is read.
const MOUNT_READS = 2
// Along the pop the ink moves one way; a frame may read this far back, the
// noise of resampling, never the half pixel a rounding moves.
const WOBBLE_PX = 0.02

test(
  "a number that pops in from larger moves one way only",
  async app => {
    let [shown, setShown] = createSignal(false)
    let stage = app.ref()
    await app.mount(() => (
      <view ref={stage} flex={1}>
        <Show when={shown()}>
          <view flexDirection="row" marginLeft={POP_AT.left} marginTop={POP_AT.top}>
            <view width={POP_BOX.width} height={POP_BOX.height} scale={1} transition={{ scale: POP }}>
              <text color="#ffffff" fontSize={NUMBER_PX}>
                128
              </text>
            </view>
            <view width={POP_BOX.width} height={POP_BOX.height} scale={1} transition={{ scale: POP }}>
              <view repaintBoundary="snapshot">
                <text color="#ffffff" fontSize={NUMBER_PX}>
                  128
                </text>
              </view>
            </view>
          </view>
        </Show>
      </view>
    ))
    await app.settle()
    setShown(true)
    let text: Ink[] = []
    let snapshot: Ink[] = []
    let rasters: number[] = []
    for (let f = 0; f < POP_FRAMES; f++) {
      let frame = await app.painted(stage)
      text.push(ink(frame, 0, POP_SPLIT_PX))
      snapshot.push(ink(frame, POP_SPLIT_PX, frame.width))
      rasters.push(textLayers(app))
    }
    for (let reads of [text, snapshot]) {
      // The frame the track settles on is the last one that moves the ink.
      let settle = POP_FRAMES - 1
      while (settle > 0 && moved(reads[settle - 1]!, reads[settle]!) === 0) settle--
      expect(settle).toBeGreaterThan(5)
      // While the track runs the ink goes right and down, never back.
      for (let f = 1; f < settle; f++) {
        expect(reads[f]!.cx).toBeGreaterThan(reads[f - 1]!.cx - WOBBLE_PX)
        expect(reads[f]!.cy).toBeGreaterThan(reads[f - 1]!.cy - WOBBLE_PX)
      }
      // The settling frame rounds once: a scale about the centre, no translate.
      expect(moved(reads[settle - 1]!, reads[settle]!)).toBeLessThanOrEqual(settleBound(reads, settle, 1))
    }
    // Both texts raster once, for the scale the pop ends at, in the frame
    // that mounts them (the count reads a frame late): nothing is left to
    // do while the scale comes down or when it rests.
    let count = (from: number) => rasters.slice(from).reduce((sum, n) => sum + n, 0)
    expect(count(0)).toBe(2)
    expect(count(MOUNT_READS)).toBe(0)
  },
  { size: POP_WINDOW, scale: POP_SCALE },
)

// ---- A translate ----

// Two filled boxes at fractions of a pixel, one above the other: the upper
// one translates under a delayed transition, the lower one is under
// pixelSnap={false}.
const BOX = { at: 10.4, width: 40.3, height: 20.3 }
const SLIDE = { duration: 100, curve: "linear", delay: 100 } as const
const SLIDE_TO = 7
// The device row between the two boxes.
const BOX_SPLIT_PX = Math.round(BOX.at + BOX.height + BOX.at / 2)
// Frames after the write: inside the delay, inside the track, and past it.
const HELD_FRAMES = 4
const RUNNING_FRAME = 9
const SETTLED_FRAME = 16

test("a box is on the grid while a translate is held, off it while the track runs, and back on it after", async app => {
  let [x, setX] = createSignal(0)
  let stage = app.ref()
  await app.mount(() => (
    <view ref={stage} flex={1}>
      <view marginLeft={BOX.at} marginTop={BOX.at} width={BOX.width} height={BOX.height} x={x()} transition={{ x: SLIDE }}>
        <d-rect color="#ffffff" />
      </view>
      <view pixelSnap={false} marginLeft={BOX.at} marginTop={BOX.at} width={BOX.width} height={BOX.height}>
        <d-rect color="#ffffff" />
      </view>
    </view>
  ))
  await app.settle()
  let rest = stage.pixels()
  expect(partial(rest, 0, BOX_SPLIT_PX)).toBe(false)
  // Under pixelSnap={false} the box keeps its fractions at rest.
  expect(partial(rest, BOX_SPLIT_PX, rest.height)).toBe(true)
  setX(SLIDE_TO)
  // A held write is not motion: the box sits still, on the grid.
  for (let f = 1; f <= HELD_FRAMES; f++) expect(partial(await app.painted(stage), 0, BOX_SPLIT_PX)).toBe(false)
  // The track runs: the box moves unrounded.
  await app.frame(RUNNING_FRAME - HELD_FRAMES - 1)
  expect(partial(await app.painted(stage), 0, BOX_SPLIT_PX)).toBe(true)
  // It settles: rounded again, in the frame that shows the end.
  await app.frame(SETTLED_FRAME - RUNNING_FRAME - 1)
  let settled = await app.painted(stage)
  expect(partial(settled, 0, BOX_SPLIT_PX)).toBe(false)
  expect(partial(settled, BOX_SPLIT_PX, settled.height)).toBe(true)
})
