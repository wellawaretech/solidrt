// A development warning for a misuse that shows later than the code that
// caused it: a layout that comes out empty, an event that never arrives. The
// site of the misuse is the component that built the thing, not the layout
// pass that notices, so the warning carries that component's stack.

/**
 * A once-only dev warning bound to the place it is created: `let warn =
 * warnOnce()` where a component builds something whose misuse surfaces
 * later (a scroller, a fill-mode scene), `warn(message)` from the layout
 * or event read that notices. The first call reports, with the stack of the
 * creation site under the message (the dev server remaps the frames to
 * .tsx); later calls are silent.
 */
export function warnOnce(): (message: string) => void {
  let origin = new Error().stack ?? ""
  let warned = false
  return message => {
    if (warned) return
    warned = true
    console.warn(`${message}\n${origin}`)
  }
}
