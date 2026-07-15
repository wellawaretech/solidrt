import { TraceMap, originalPositionFor } from "@jridgewell/trace-mapping"

// Stack-trace remapping for forwarded client logs. The runtime evaluates the
// bundle as module "main", so QuickJS frames cite bundle positions like
// "at boom (main:212:9)". With the current reload's sourcemap latched on the
// server (state.currentMap), those positions are rewritten to the original
// .tsx sources before a log entry is buffered.

// The parsed map is cached per map text; a reload swaps the text and the next
// lookup rebuilds the tracer.
let cachedText: string | null = null
let tracer: TraceMap | null = null

function tracerFor(map: string | null): TraceMap | null {
  if (map !== cachedText) {
    cachedText = map
    tracer = null
    if (map) {
      try {
        tracer = new TraceMap(JSON.parse(map))
      } catch {
        // A malformed map disables remapping until the next reload.
      }
    }
  }
  return tracer
}

/**
 * Rewrite every "main:LINE:COL" (or "main:LINE") position in `text` to its
 * original source position, e.g. "src/app.tsx:42:7". Positions the map has no
 * entry for, and all text when `map` is null, pass through unchanged.
 * QuickJS lines and columns are 1-based; sourcemap columns are 0-based.
 */
export function remapPositions(text: string, map: string | null): string {
  if (!map || !text.includes("main:")) return text
  let t = tracerFor(map)
  if (!t) return text
  return text.replace(/\bmain:(\d+)(?::(\d+))?\b/g, (frame, line, column) => {
    let pos = originalPositionFor(t, {
      line: parseInt(line, 10),
      column: column ? Math.max(parseInt(column, 10) - 1, 0) : 0,
    })
    if (pos.source == null || pos.line == null) return frame
    return `${pos.source}:${pos.line}:${(pos.column ?? 0) + 1}`
  })
}
