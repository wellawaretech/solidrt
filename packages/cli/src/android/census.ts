// `sol android --census`: the SurfaceFlinger present census, engine-free.
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
import { chooseLayer, INTERVAL_BIN_MAX, layerName, summarize, type Census } from "./surfaceflinger"

let sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

function sh(adb: string, target: string, args: string[]): string {
  let res = Bun.spawnSync([adb, "-s", target, "shell", ...args], { stdout: "pipe", stderr: "pipe" })
  if (res.exitCode !== 0) {
    console.error(`adb shell ${args.join(" ")} failed:\n${res.stderr.toString()}`)
    process.exit(1)
  }
  return res.stdout.toString().replace(/\r/g, "")
}

// The layer's frame history: the refresh period, then one row per history
// slot (queued, present, frameReady).
function latency(adb: string, target: string, layer: string): string {
  return sh(adb, target, ["dumpsys", "SurfaceFlinger", "--latency", `'${layer}'`])
}

// The layer to census (surfaceflinger.ts chooseLayer) among the candidates:
// the layers `--layer` matches as a substring, else the layers of the first
// of `packages` (the package followed by its activity) with any on screen.
// Exits naming the layers when nothing matches.
function resolveLayer(adb: string, target: string, packages: string[]): string {
  let layers = sh(adb, target, ["dumpsys", "SurfaceFlinger", "--list"])
    .split("\n")
    .map((l) => layerName(l.trim()))
    .filter(Boolean)
  let candidates = values.layer
    ? layers.filter((l) => l.includes(values.layer!))
    : (packages.map((pkg) => layers.filter((l) => l.includes(pkg + "/"))).find((c) => c.length > 0) ?? [])
  if (candidates.length === 0) {
    let asked = values.layer ? `--layer "${values.layer}"` : `package ${packages.join(" or ")}`
    console.error(`No SurfaceFlinger layer matches ${asked}; is the app on screen? Layers:`)
    for (let l of layers) console.error(`  ${l}`)
    process.exit(1)
  }
  return chooseLayer(candidates, (layer) => latency(adb, target, layer))
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
export async function census(adb: string, target: string, packages: string[]) {
  let layer = resolveLayer(adb, target, packages)
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
  let summary = summarize(layer, latency(adb, target, layer))
  if (values.json) {
    console.log(JSON.stringify(summary))
  } else {
    print(summary)
  }
}
