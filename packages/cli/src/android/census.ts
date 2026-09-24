// `srt android --census`: the SurfaceFlinger present census, engine-free.
// The compositor keeps a short per-layer history of frame timestamps
// (`dumpsys SurfaceFlinger --latency <layer>`): for every buffer the app
// queued, the time it was queued (the "desired present" column when the app
// requests no presentation time, which the runtime never does), the vsync
// the frame was shown on, and the time its rendering finished (the frameReady
// fence). Two figures come out of it that no runtime counter can be wrong
// about: the present interval in refreshes (the cadence the panel actually
// showed) and the GPU span, frameReady minus queue, the frame's own GPU work
// on a tiler where the queue is the submission. Every Android pacing and
// paint-cost verdict is measured with this; it used to be retyped as a
// scratch script per device session.

import { values } from "../lib/args"

let sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

// An unfilled history slot: SurfaceFlinger reports 0 for a column it has no
// value for and INT64_MAX for a present that has not happened.
const UNSET = 0n
const NEVER = 9223372036854775807n

// Present intervals this many refreshes and longer are one histogram bin:
// past it the frame count is what matters, not the exact length.
const INTERVAL_BIN_MAX = 4

type Row = { queued: bigint; present: bigint; ready: bigint }

export type Census = {
  layer: string
  periodMs: number
  presents: number
  spanS: number
  /// Present interval in refreshes, rounded: index 1..INTERVAL_BIN_MAX (the
  /// last bin holds everything at or past it), count per bin.
  intervals: number[]
  intervalMsP50: number
  intervalMsP90: number
  /// frameReady minus queue time, per present.
  gpuMsP50: number
  gpuMsP90: number
  gpuMsMax: number
}

function sh(adb: string, target: string, args: string[]): string {
  let res = Bun.spawnSync([adb, "-s", target, "shell", ...args], { stdout: "pipe", stderr: "pipe" })
  if (res.exitCode !== 0) {
    console.error(`adb shell ${args.join(" ")} failed:\n${res.stderr.toString()}`)
    process.exit(1)
  }
  return res.stdout.toString().replace(/\r/g, "")
}

// The layer to census: `--layer` matched as a substring of the compositor's
// layer list, else the client's own SurfaceView layer (the BLAST layer that
// carries its buffers, which is what --latency reports on; the activity's
// other layers hold none and would census as zero presents). Exits naming
// the candidates when none matches.
function resolveLayer(adb: string, target: string, packageName: string): string {
  let layers = sh(adb, target, ["dumpsys", "SurfaceFlinger", "--list"])
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
  let pick = (test: (l: string) => boolean) => layers.find(test)
  // The BLAST child is the one with buffers; its parent SurfaceView layer
  // (listed first) holds none. Older releases without BLAST name the
  // buffer layer plainly.
  let surface = (l: string) => l.startsWith("SurfaceView") && l.includes(packageName)
  let layer = values.layer
    ? pick((l) => l.includes(values.layer!))
    : (pick((l) => surface(l) && l.includes("(BLAST)")) ?? pick(surface))
  if (layer) return layer
  let asked = values.layer ? `--layer "${values.layer}"` : `package ${packageName}`
  console.error(`No SurfaceFlinger layer matches ${asked}; is the app on screen? Layers:`)
  for (let l of layers) console.error(`  ${l}`)
  process.exit(1)
}

function parseLatency(text: string): { periodNs: bigint; rows: Row[] } {
  let lines = text.split("\n").map((l) => l.trim()).filter(Boolean)
  let periodNs = BigInt(lines[0] ?? "0")
  let rows: Row[] = []
  for (let line of lines.slice(1)) {
    let cols = line.split(/\s+/)
    if (cols.length < 3) continue
    let [queued, present, ready] = cols.map((c) => BigInt(c)) as [bigint, bigint, bigint]
    if (queued === UNSET || present === UNSET || present === NEVER || ready === UNSET || ready === NEVER) continue
    rows.push({ queued, present, ready })
  }
  rows.sort((a, b) => (a.present < b.present ? -1 : a.present > b.present ? 1 : 0))
  return { periodNs, rows }
}

let ms = (ns: bigint) => Number(ns) / 1e6

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0
  return sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * p))]!
}

export function summarize(layer: string, text: string): Census {
  let { periodNs, rows } = parseLatency(text)
  let periodMs = ms(periodNs)
  let intervals = new Array<number>(INTERVAL_BIN_MAX + 1).fill(0)
  let intervalMs: number[] = []
  for (let i = 1; i < rows.length; i++) {
    let delta = ms(rows[i]!.present - rows[i - 1]!.present)
    intervalMs.push(delta)
    let refreshes = periodMs > 0 ? Math.max(1, Math.round(delta / periodMs)) : 1
    intervals[Math.min(refreshes, INTERVAL_BIN_MAX)]! += 1
  }
  let gpuMs = rows.map((r) => ms(r.ready - r.queued)).sort((a, b) => a - b)
  intervalMs.sort((a, b) => a - b)
  let spanS = rows.length >= 2 ? ms(rows[rows.length - 1]!.present - rows[0]!.present) / 1000 : 0
  return {
    layer,
    periodMs,
    presents: rows.length,
    spanS,
    intervals,
    intervalMsP50: percentile(intervalMs, 0.5),
    intervalMsP90: percentile(intervalMs, 0.9),
    gpuMsP50: percentile(gpuMs, 0.5),
    gpuMsP90: percentile(gpuMs, 0.9),
    gpuMsMax: gpuMs[gpuMs.length - 1] ?? 0,
  }
}

function print(c: Census) {
  let bins = c.intervals
    .map((n, i) => (i === 0 ? null : `${i}${i === INTERVAL_BIN_MAX ? "+" : ""}: ${n}`))
    .filter(Boolean)
    .join("  ")
  console.log(`layer: ${c.layer}`)
  console.log(`refresh period ${c.periodMs.toFixed(2)} ms; ${c.presents} presents over ${c.spanS.toFixed(2)} s`)
  console.log(`present interval (refreshes): ${bins}`)
  console.log(`present interval p50 ${c.intervalMsP50.toFixed(1)} ms, p90 ${c.intervalMsP90.toFixed(1)} ms`)
  console.log(
    `gpu span (frameReady - queue) p50 ${c.gpuMsP50.toFixed(1)} ms, p90 ${c.gpuMsP90.toFixed(1)} ms, max ${c.gpuMsMax.toFixed(1)} ms`,
  )
}

// Run the census on one device: clear the compositor's history first on
// --clear, wait --seconds while the person (or an agent over the control
// API) drives the app, then read and summarize the layer's history. Without
// --clear the history is whatever the compositor kept (its ring is short, a
// few seconds of presents), which is the "what just happened" reading.
export async function census(adb: string, target: string, packageName: string) {
  let layer = resolveLayer(adb, target, packageName)
  if (values.clear) {
    sh(adb, target, ["dumpsys", "SurfaceFlinger", "--latency-clear"])
  }
  let seconds = values.seconds ? Number(values.seconds) : 0
  if (!Number.isFinite(seconds) || seconds < 0) {
    console.error(`--seconds must be a non-negative number, got "${values.seconds}"`)
    process.exit(1)
  }
  if (seconds > 0) {
    // Progress goes to stderr: with --json, stdout is the summary alone.
    console.error(`[cli] Collecting ${seconds} s of presents on ${layer}`)
    await sleep(seconds * 1000)
  }
  let text = sh(adb, target, ["dumpsys", "SurfaceFlinger", "--latency", `'${layer}'`])
  let summary = summarize(layer, text)
  if (values.json) {
    console.log(JSON.stringify(summary))
  } else {
    print(summary)
  }
}
