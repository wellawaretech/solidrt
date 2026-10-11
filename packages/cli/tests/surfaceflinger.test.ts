// How `sol android --census` reads SurfaceFlinger's dumps
// (src/android/surfaceflinger.ts): the layer list in both of its forms, the
// pick among an app's layers, and the census over a history. The names are
// captured from a tablet on Android 12 and a Pixel 7 on Android 17.
import { test, expect } from "flux:test"
import { chooseLayer, layerName, summarize } from "../src/android/surfaceflinger.ts"

// Android 12: `<name>#<id>` lines.
const PLAIN = [
  "ActivityRecord{2aa1467 u0 com.solidrt.player.dev/com.solidrt.app.MainActivity t1079}#0",
  "762f2ca com.solidrt.player.dev/com.solidrt.app.MainActivity#0",
  "Background for SurfaceView - com.solidrt.player.dev/com.solidrt.app.MainActivity@5787fbc@0#0",
  "SurfaceView - com.solidrt.player.dev/com.solidrt.app.MainActivity@5787fbc@0#0",
  "SurfaceView - com.solidrt.player.dev/com.solidrt.app.MainActivity@5787fbc@0(BLAST)#0",
  "com.solidrt.player.dev/com.solidrt.app.MainActivity$_23140#0",
  "Bounds for - com.solidrt.player.dev/com.solidrt.app.MainActivity@0#0",
]
const PLAIN_BLAST = PLAIN[4]!

// Android 17: `RequestedLayerState{<name>#<id> key=value ...}` records.
const RECORDS = [
  "RequestedLayerState{e44114c ActivityRecordInputSink com.wellawaretech.vision/com.solidrt.app.MainActivity#94181 parentId=94175 z=-2147483648}",
  "RequestedLayerState{VRI-com.wellawaretech.vision/com.solidrt.app.MainActivity#94184 parentId=94183}",
  "RequestedLayerState{Background for ed85e44 SurfaceView[com.wellawaretech.vision/com.solidrt.app.MainActivity]#94190 parentId=94188 relativeParentId=94184 z=-2147483648}",
  "RequestedLayerState{ed85e44 SurfaceView[com.wellawaretech.vision/com.solidrt.app.MainActivity](BLAST)#94189 parentId=94188}",
  "RequestedLayerState{44412e5 com.wellawaretech.vision/com.solidrt.app.MainActivity#94183 parentId=94175}",
  "RequestedLayerState{ActivityRecord{218674581 u0 com.wellawaretech.vision/com.solidrt.app.MainActivity t37588}#94175 parentId=94174}",
  "RequestedLayerState{ed85e44 SurfaceView[com.wellawaretech.vision/com.solidrt.app.MainActivity]#94188 parentId=94187 relativeParentId=94184 z=-1}",
  "RequestedLayerState{Bounds for - com.wellawaretech.vision/com.solidrt.app.MainActivity#94187 parentId=94184}",
]
const RECORD_BLAST = "ed85e44 SurfaceView[com.wellawaretech.vision/com.solidrt.app.MainActivity](BLAST)#94189"

// A history: the refresh period, then rows of queued, present, frameReady in
// nanoseconds. 0 is a column without a value, INT64_MAX a present that has
// not happened.
const PERIOD_NS = 16666666n
const NEVER = "9223372036854775807"
function history(rows: Array<[bigint, bigint, bigint]>, unsetSlots = 0): string {
  let lines = [String(PERIOD_NS)]
  for (let i = 0; i < unsetSlots; i++) lines.push("0\t0\t0")
  for (let [queued, present, ready] of rows) lines.push(`${queued}\t${present}\t${ready}`)
  return lines.join("\n") + "\n"
}
// `n` presents one refresh apart, each queued 3 ms before its present and
// ready 1 ms after queueing.
function presents(n: number): Array<[bigint, bigint, bigint]> {
  let rows: Array<[bigint, bigint, bigint]> = []
  for (let i = 0n; i < BigInt(n); i++) {
    let present = 1_000_000_000n + i * PERIOD_NS
    rows.push([present - 3_000_000n, present, present - 2_000_000n])
  }
  return rows
}

test("a plain list line is the layer name", () => {
  for (let line of PLAIN) expect(layerName(line)).toBe(line)
})

test("a record's name comes out of the braces, whatever it holds", () => {
  expect(layerName(RECORDS[3]!)).toBe(RECORD_BLAST)
  // several attributes, a negative value
  expect(layerName(RECORDS[2]!)).toBe("Background for ed85e44 SurfaceView[com.wellawaretech.vision/com.solidrt.app.MainActivity]#94190")
  // braces and spaces in the name itself
  expect(layerName(RECORDS[5]!)).toBe("ActivityRecord{218674581 u0 com.wellawaretech.vision/com.solidrt.app.MainActivity t37588}#94175")
  // key=value text inside the name is not an attribute
  expect(layerName("RequestedLayerState{WindowToken{a669423 type=2000 android.os.BinderProxy@42189dd}#92 parentId=15}")).toBe(
    "WindowToken{a669423 type=2000 android.os.BinderProxy@42189dd}#92",
  )
  expect(layerName("RequestedLayerState{Root#0}")).toBe("Root#0")
})

test("the layer with presents in its history is the one", () => {
  let names = RECORDS.map(layerName)
  let read: string[] = []
  let histories = (layer: string) => {
    read.push(layer)
    return layer === RECORD_BLAST ? history(presents(3)) : history([])
  }
  expect(chooseLayer(names, histories)).toBe(RECORD_BLAST)
  expect(read.length).toBe(names.length)
  // the same with the convention pointing elsewhere: the history decides
  let other = names[1]!
  expect(chooseLayer(names, (layer) => (layer === other ? history(presents(2)) : history([], 127)))).toBe(other)
})

test("without presents anywhere, the BLAST SurfaceView, then a SurfaceView, then the first", () => {
  let none = () => history([], 127)
  expect(chooseLayer(RECORDS.map(layerName), none)).toBe(RECORD_BLAST)
  expect(chooseLayer(PLAIN, none)).toBe(PLAIN_BLAST)
  expect(chooseLayer(PLAIN.filter((l) => !l.includes("(BLAST)")), none)).toBe(PLAIN[3])
  expect(chooseLayer([PLAIN[0]!, PLAIN[1]!], none)).toBe(PLAIN[0])
})

test("a single candidate is taken without reading a history", () => {
  expect(
    chooseLayer([PLAIN[0]!], () => {
      throw new Error("read")
    }),
  ).toBe(PLAIN[0])
})

test("the census counts the filled rows, unset slots or none", () => {
  let five = summarize("x", history(presents(5), 122))
  expect(five.presents).toBe(5)
  expect(five.periodMs).toBeCloseTo(16.67, 2)
  expect(five.intervals).toEqual([0, 4, 0, 0, 0])
  expect(five.gpuMsP50).toBeCloseTo(1, 6)
  // a present that has not happened yet is not a row
  let pending = summarize("x", history([...presents(2), [1n, BigInt(NEVER), 0n]]))
  expect(pending.presents).toBe(2)
  // Android 15 and later print no slots for a layer nothing was presented on
  let empty = summarize("x", `${PERIOD_NS}\n`)
  expect(empty.presents).toBe(0)
  expect(empty.spanS).toBe(0)
})
