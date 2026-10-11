// SurfaceFlinger's dumps as `sol android --census` reads them (census.ts):
// the layer list, a layer's frame history, and the census arithmetic over
// it. No adb and no arguments here, so a captured dump tests the lot.

// An unfilled history slot: SurfaceFlinger reports 0 for a column it has no
// value for and INT64_MAX for a present that has not happened.
const UNSET = 0n
const NEVER = 9223372036854775807n

// Present intervals this many refreshes and longer are one histogram bin:
// past it the frame count is what matters, not the exact length.
export const INTERVAL_BIN_MAX = 4

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

// A line of `dumpsys SurfaceFlinger --list` as `--latency` addresses the
// layer: `<name>#<id>` plainly up to Android 14; Android 15 and later print
// `RequestedLayerState{<name>#<id> parentId=.. z=..}` records, which
// --latency does not answer to, so the name comes out of the braces.
const LAYER_RECORD = /^RequestedLayerState\{(.+?)((?: [A-Za-z]+=[^ ]*)*)\}$/

export function layerName(line: string): string {
  let record = LAYER_RECORD.exec(line)
  return record ? record[1]! : line
}

// The layer to census among `candidates`, the layers that matched. An app
// has several: its activity record, its input sink, the SurfaceView and its
// BLAST child, the bounds and background layers. Only the one carrying its
// buffers has a present history, and every other would census as zero
// presents, so the compositor's own answer decides first: the candidates
// whose history (`history`, the --latency dump of a layer) holds presents.
// The name convention is the tie-break, for candidates that have not
// presented yet: the SurfaceView's BLAST child, then a SurfaceView (older
// releases without BLAST name the buffer layer plainly), then the first
// match. A single candidate is taken without a read.
export function chooseLayer(candidates: string[], history: (layer: string) => string): string {
  if (candidates.length > 1) {
    let presenting = candidates.filter((l) => parseLatency(history(l)).rows.length > 0)
    if (presenting.length > 0) candidates = presenting
  }
  let surface = (l: string) => SURFACE_VIEW.test(l)
  return candidates.find((l) => surface(l) && l.includes("(BLAST)")) ?? candidates.find(surface) ?? candidates[0]!
}

// A SurfaceView's own layer: named `SurfaceView - <activity>...` up to
// Android 14, `<hex hash> SurfaceView[<activity>]...` from 15. Its "Bounds
// for" and "Background for" siblings carry the words too, but not first.
const SURFACE_VIEW = /^(?:[0-9a-f]+ )?SurfaceView[ []/

// The refresh period and the filled history rows. A row is filled once its
// present happened and its fence signalled; the rest are unset slots (older
// releases print every slot of the ring, Android 15 and later only the
// recorded frames, so a layer nothing was presented on has no rows either
// way).
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

// The census of a layer's history.
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
