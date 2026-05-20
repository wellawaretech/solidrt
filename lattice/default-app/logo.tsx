import { onRender, onResize } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"

type LogoColors = {
  dark: string
  mid: string
  light: string
}

type Shade = "dark" | "mid" | "light"

const SOLID_COLORS: LogoColors = {
  dark: "rgba(26,51,128)",
  mid: "rgba(51,102,179)",
  light: "rgba(102,153,230)",
}

const RT_COLORS: LogoColors = {
  dark: "rgba(100,100,100)",
  mid: "rgba(140,140,140)",
  light: "rgba(180,180,180)",
}

const M = 25
const R = M * Math.SQRT2
const T = -0.5 * R //left shift for "t"

type Point = [number, number]

let sq: Point[] = [
  [0, 0],
  [2 * M, 0],
  [2 * M, 2 * M],
  [0, 2 * M],
]
let tri1: Point[] = [
  [0, 0],
  [2 * M, 0],
  [0, 2 * M],
]
let tri2: Point[] = [
  [0, 0],
  [2 * R, 0],
  [0, 2 * R],
]
let tri3: Point[] = [
  [0, 0],
  [4 * M, 0],
  [0, 4 * M],
]
let par1: Point[] = [
  [0, 0],
  [2 * M, 0],
  [4 * M, 2 * M],
  [2 * M, 2 * M],
]
let par2: Point[] = [
  [2 * M, 0],
  [4 * M, 0],
  [2 * M, 2 * M],
  [0, 2 * M],
]

function shapeCenter(shape: Point[], rotate: number): Point {
  let radians = (rotate * Math.PI) / 4
  let cos = Math.cos(radians)
  let sin = Math.sin(radians)

  let pts: Point[] = shape.map(([x, y]) => [x * cos - y * sin, x * sin + y * cos])
  let minX = Math.min(...pts.map(([x]) => x))
  let minY = Math.min(...pts.map(([, y]) => y))
  pts = pts.map(([x, y]) => [x - minX, y - minY])

  let area = 0
  let cx = 0
  let cy = 0
  for (let i = 0; i < pts.length; i++) {
    let [x0, y0] = pts[i]!
    let [x1, y1] = pts[(i + 1) % pts.length]!
    let cross = x0 * y1 - x1 * y0
    area += cross
    cx += (x0 + x1) * cross
    cy += (y0 + y1) * cross
  }
  area *= 0.5
  cx /= 6 * area
  cy /= 6 * area

  return [cx, cy]
}

type Piece = { shape: Point[]; x: number; y: number; rot: number; shade: Shade }
type Letter = { width: number; height: number; scale?: number; pieces: Piece[] }

function path(shape: Point[], rotate: number) {
  let radians = (rotate * Math.PI) / 4
  let cos = Math.cos(radians)
  let sin = Math.sin(radians)

  let rotated: Point[] = shape.map(([x, y]) => [x * cos - y * sin, x * sin + y * cos])

  let minX = Math.min(...rotated.map(([x]) => x))
  let minY = Math.min(...rotated.map(([, y]) => y))

  let d = "M" + rotated.map(([x, y]) => `${x - minX} ${y - minY}`).join("L") + "Z"

  return d
}

let letters: Letter[] = [
  {
    // S
    width: 5 * R,
    height: 6 * R,
    pieces: [
      { shape: tri1, x: R, y: 5 * R, rot: 1, shade: "light" },
      { shape: sq, x: 0, y: 4 * R, rot: 1, shade: "mid" },
      { shape: tri1, x: 2 * R, y: 4 * R, rot: -1, shade: "dark" },
      { shape: tri3, x: 3 * R, y: 2 * R, rot: 3, shade: "mid" },
      { shape: tri3, x: R, y: 0, rot: -1, shade: "dark" },
      { shape: tri2, x: 3 * R, y: 0, rot: 0, shade: "mid" },
      { shape: par2, x: 5 * R - 2 * M, y: 0, rot: 0, shade: "light" },
    ],
  },
  {
    // O
    width: 4 * R + 2 * M,
    height: 2 * M + 4 * R,
    pieces: [
      { shape: tri3, x: 0, y: 2 * M, rot: -1, shade: "dark" },
      { shape: sq, x: 2 * R, y: 4 * R, rot: 0, shade: "light" },
      { shape: tri3, x: 2 * R + 2 * M, y: 0, rot: 3, shade: "mid" },
      { shape: tri1, x: 2 * R - 2 * M, y: 2 * M, rot: 0, shade: "mid" },
      { shape: tri1, x: 2 * R, y: 0, rot: 2, shade: "dark" },
      { shape: par1, x: 2 * R + 2 * M, y: 4 * R - 2 * M, rot: -2, shade: "dark" },
      { shape: tri2, x: 2 * R - 2 * M, y: 0, rot: 1, shade: "light" },
    ],
  },
  {
    // L
    width: 4 * M + 2 * R,
    height: 4 * M + 4 * R,
    pieces: [
      { shape: sq, x: 2 * R - 2 * M, y: 0, rot: 0, shade: "light" },
      { shape: tri1, x: 2 * R - 2 * M, y: 2 * M, rot: 0, shade: "mid" },
      { shape: tri3, x: 0, y: 2 * M, rot: -1, shade: "dark" },
      { shape: tri3, x: 2 * R - 2 * M, y: 4 * R, rot: -2, shade: "mid" },
      { shape: par1, x: 2 * R, y: 4 * R + 2 * M, rot: 0, shade: "dark" },
      { shape: tri2, x: 4 * M, y: 2 * R + 4 * M, rot: 2, shade: "mid" },
      { shape: tri1, x: 4 * M, y: R + 4 * M, rot: 1, shade: "light" },
    ],
  },
  {
    // I
    width: 6 * M,
    height: 8 * M,
    pieces: [
      { shape: sq, x: 4 * M, y: 0, rot: 0, shade: "dark" },
      { shape: tri3, x: 0, y: 0, rot: 0, shade: "light" },
      { shape: par2, x: 2 * M, y: 2 * M, rot: -2, shade: "light" },
      { shape: tri2, x: 2 * M, y: 0, rot: -1, shade: "mid" },
      { shape: tri3, x: 2 * M, y: 4 * M, rot: -2, shade: "dark" },
      { shape: tri1, x: 0, y: 6 * M, rot: 4, shade: "mid" },
      { shape: tri1, x: 4 * M, y: 6 * M, rot: 2, shade: "mid" },
    ],
  },
  {
    // D
    width: 6 * M,
    height: 8 * M,
    pieces: [
      { shape: tri3, x: 0, y: 0, rot: 0, shade: "mid" },
      { shape: tri3, x: 0, y: 4 * M, rot: -2, shade: "dark" },
      { shape: tri1, x: 2 * M, y: 0, rot: 4, shade: "dark" },
      { shape: par2, x: 4 * M, y: 0, rot: 2, shade: "light" },
      { shape: tri1, x: 4 * M, y: 2 * M, rot: -2, shade: "dark" },
      { shape: sq, x: 4 * M, y: 4 * M, rot: 0, shade: "light" },
      { shape: tri2, x: 2 * M, y: 6 * M, rot: -3, shade: "mid" },
    ],
  },
// {
//   // -
//   width: 8 * M,
//   height: 8 * M,
//   scale: 0.5,
//   pieces: [
//     { shape: tri3, x: 0, y: 0, rot: 4, shade: "dark" },
//     { shape: tri3, x: 0, y: 4 * M, rot: 2, shade: "mid" },
//     { shape: tri1, x: 4 * M, y: 0, rot: -2, shade: "light" },
//     { shape: sq, x: 4 * M, y: 2 * M, rot: 0, shade: "mid" },
//     { shape: tri1, x: 4 * M, y: 4 * M, rot: 0, shade: "light" },
//     { shape: par1, x: 4 * M, y: 4 * M, rot: 2, shade: "dark" },
//     { shape: tri2, x: 6 * M, y: 2 * M, rot: 3, shade: "light" },
//   ],
// },
  {
    // R
    width: 6 * M,
    height: 8 * M,
    pieces: [
      { shape: tri3, x: 0, y: 0, rot: 0, shade: "mid" },
      { shape: tri3, x: 0, y: 4 * M, rot: 0, shade: "dark" },
      { shape: tri2, x: 2 * M, y: 0, rot: 1, shade: "dark" },
      { shape: sq, x: 4 * M - R, y: 4 * M, rot: 1, shade: "light" },
      { shape: tri1, x: 0, y: 6 * M, rot: 4, shade: "light" },
      { shape: tri1, x: 4 * M, y: 4 * M + R, rot: -1, shade: "mid" },
      { shape: par2, x: 2 * M, y: 2 * M, rot: 0, shade: "mid" },
    ],
  },
  {
    // T
    width: 6 * M,
    height: 4 * M + 4 * R,
    pieces: [
      { shape: par1, x: T + 2 * R - 2 * M, y: 0, rot: -2, shade: "light" },
      { shape: tri1, x: T + 2 * R - 2 * M, y: 0, rot: 0, shade: "mid" },
      { shape: tri3, x: T + 0, y: 2 * M, rot: -1, shade: "dark" },
      { shape: tri3, x: T + 2 * R - 2 * M, y: 4 * R, rot: -2, shade: "mid" },
      { shape: tri2, x: T + 2 * R, y: 2 * M, rot: -3, shade: "light" },
      { shape: tri1, x: T + 2 * R, y: 2 * M, rot: -2, shade: "mid" },
      { shape: sq, x: T + 2 * M + R, y: 4 * M + 2 * R, rot: 1, shade: "dark" },
    ],
  },
]

const EXPLODE_DIST = 10

const STAGGER_DELAY = 50
const ANIM_DURATION = 600
const HOLD_ASSEMBLED = 5000
const HOLD_EXPLODED = 0

function TangramLetter(props: { letter: Letter; colors: LogoColors; delay: number }) {
  let [dist, setDist] = createSignal(EXPLODE_DIST)
  let start: number = null!

  let letterCx = props.letter.width / 2
  let letterCy = props.letter.height / 2

  let pieceVectors = props.letter.pieces.map((p) => {
    let [scx, scy] = shapeCenter(p.shape, p.rot)
    return [p.x + scx - letterCx, p.y + scy - letterCy] as Point
  })

  let pieceSpins = props.letter.pieces.map((_, i) => (((i * 7 + 3) % 11) - 5) * 30)

  onRender((_tick: number) => {
    if (start === null) start = _tick
    let tick = _tick - start

    // same cycle length for all letters; delay only offsets the start
    let cycleLen = ANIM_DURATION + HOLD_ASSEMBLED + ANIM_DURATION + HOLD_EXPLODED
    let t = (tick - props.delay) % cycleLen

    if (t < 0) {
      // before this letter's first cycle starts
      setDist(EXPLODE_DIST)
    } else if (t < ANIM_DURATION) {
      // falling into place
      let p = t / ANIM_DURATION
      let ease = p * p * (3 - 2 * p)
      setDist((1 - ease) * EXPLODE_DIST)
    } else if (t < ANIM_DURATION + HOLD_ASSEMBLED) {
      // holding assembled
      setDist(0)
    } else if (t < 2 * ANIM_DURATION + HOLD_ASSEMBLED) {
      // exploding out
      let p = (t - ANIM_DURATION - HOLD_ASSEMBLED) / ANIM_DURATION
      let ease = p * p * (3 - 2 * p)
      setDist(ease * EXPLODE_DIST)
    } else {
      // holding exploded
      setDist(EXPLODE_DIST)
    }
  })

  return (
    <view
      width={props.letter.width}
      height={props.letter.height}
      scale={props.letter.scale}
    >
      {props.letter.pieces.map((p, i) => (
        <view
          x={pieceVectors[i]![0] * dist()}
          y={pieceVectors[i]![1] * dist()}
          scale={1 + dist() * 0.5}
          rotate={(pieceSpins[i]! * dist()) / EXPLODE_DIST / 150}
        >
          <d-path color={props.colors[p.shade]} x={p.x} y={p.y} d={path(p.shape, p.rot)} />
        </view>
      ))}
    </view>
  )
}

export function Logo() {
  let [scale, setScale] = createSignal(1)

  onResize(({ width }) => {
    setScale((width * 0.8) / 1500)
  })

  return (
    <view justifyContent="center" width={1500} scale={scale()}>
      <view gap={50} flexDirection="row" alignItems="flex-end">
        {letters.map((letter, i) => (
          <TangramLetter letter={letter} colors={i < 5 ? SOLID_COLORS : RT_COLORS} delay={i * STAGGER_DELAY} />
        ))}
      </view>
    </view>
  )
}
