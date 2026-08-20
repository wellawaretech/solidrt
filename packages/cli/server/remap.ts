import { TraceMap, originalPositionFor } from "@jridgewell/trace-mapping"

// Stack-trace remapping for forwarded client logs. The runtime evaluates the
// app bundle as module "main" and each isolate bundle under its isolate id,
// so QuickJS frames cite bundle positions like "at boom (main:212:9)" or
// "at boom (worker:65:13)". With the current reload's sourcemaps latched on
// the server (state.currentMaps, keyed by module name), those positions are
// rewritten to the original .tsx sources before a log entry is buffered. A
// module without a map is left as it is: a position is never remapped against
// another module's map.

// Parsed maps are cached per module name and map text; a reload swaps the
// texts and the next lookup rebuilds the tracers. Entries for removed modules
// linger unused, which is harmless.
let cached = new Map<string, { text: string; tracer: TraceMap | null }>()

function tracerFor(name: string, text: string): TraceMap | null {
  let entry = cached.get(name)
  if (!entry || entry.text !== text) {
    let tracer: TraceMap | null = null
    try {
      tracer = new TraceMap(JSON.parse(text))
    } catch {
      // A malformed map disables remapping for this module until the next reload.
    }
    entry = { text, tracer }
    cached.set(name, entry)
  }
  return entry.tracer
}

let REGEX_SPECIALS = /[.*+?^${}()|[\]\\]/g

/**
 * Rewrite every "NAME:LINE:COL" (or "NAME:LINE") position in `text`, for each
 * module NAME in `maps`, to its original source position, e.g.
 * "src/app.tsx:42:7". Positions a map has no entry for, positions of modules
 * without a map, and all text when `maps` is null, pass through unchanged.
 * QuickJS lines and columns are 1-based; sourcemap columns are 0-based.
 */
export function remapPositions(text: string, maps: Record<string, string> | null): string {
  if (!maps) return text
  for (let [name, map] of Object.entries(maps)) {
    if (!text.includes(name + ":")) continue
    let t = tracerFor(name, map)
    if (!t) continue
    // The leading capture keeps a name from matching inside a longer one
    // ("audio" inside "workers/audio" or "app" inside "src/app.tsx").
    let pattern = new RegExp(`(^|[^\\w/.$-])${name.replace(REGEX_SPECIALS, "\\$&")}:(\\d+)(?::(\\d+))?\\b`, "g")
    text = text.replace(pattern, (frame, prefix, line, column) => {
      let pos = originalPositionFor(t, {
        line: parseInt(line, 10),
        column: column ? Math.max(parseInt(column, 10) - 1, 0) : 0,
      })
      if (pos.source == null || pos.line == null) return frame
      return `${prefix}${pos.source}:${pos.line}:${(pos.column ?? 0) + 1}`
    })
  }
  return text
}
