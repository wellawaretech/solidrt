// A leaf's on-screen box, followed: measured after every layout and again
// when the display scale changes (which lays nothing out), handed to
// `apply` in the leaf's own units and in device pixels. The mechanism
// under a draw target that sizes to the element showing it - a 3d scene
// or a 2d view filling its pane, a shader backdrop behind a card - so
// that the follow and its two warnings are written once, and each
// consumer keeps only its size decision.
import { createEffect, untrack } from "@solidjs/signals"
import { getBoundingBoxViewport, getLayoutBox } from "./core"
import { displayScale, onLayout } from "./window"
import { warnOnce } from "./warn"

/** What followBox hands its apply: the leaf's box two ways. */
export type FollowedBox = {
  /** The leaf's laid-out box in ITS units (getLayoutBox): what pointer
   * localX/localY report in, and a 2d view's pixel space. */
  layout: { width: number; height: number }
  /** The leaf's on-screen box in device pixels (getBoundingBoxViewport
   * times the display scale), unrounded: display scale, designSize fits
   * and ancestor transforms composed - the size a target renders at to
   * show one texel per device pixel. */
  device: { width: number; height: number }
  /** The display scale the device box used. */
  scale: number
}

/**
 * Follow the leaf `leaf()` returns (undefined until it mounts): `apply`
 * runs with its measured box after every layout, and again when the
 * display scale or `deps` change. Both runs are untracked; `deps` is the
 * tracked read of whatever else the decision depends on (a scale prop, a
 * budget). A leaf laid out at zero size warns once, naming `what` (it
 * adds no size of its own under `contain="size"`, so a parent with no
 * size leaves it empty and nothing on screen says why); a leaf that
 * never arrives warns once with `missing`, when given (the case of an
 * app-composed leaf that was never ref'd). Registered on the current
 * owner, like onLayout.
 */
export function followBox(
  what: string,
  leaf: () => { id: number } | undefined,
  apply: (box: FollowedBox) => void,
  opts?: { deps?: () => unknown; missing?: string },
): void {
  // One instance for both warnings: the stack it captures is the
  // component that set the follow up, and a follow that warns twice
  // says nothing the first warning did not.
  let warn = warnOnce()
  let run = () =>
    untrack(() => {
      let node = leaf()
      if (!node) {
        if (opts?.missing) warn(`${what}: ${opts.missing}`)
        return
      }
      let viewport = getBoundingBoxViewport(node)
      let layout = getLayoutBox(node)
      if (!viewport || !layout) return
      if (viewport.width === 0 || viewport.height === 0) {
        warn(
          `${what}: the followed leaf laid out ${viewport.width} x ${viewport.height}, so nothing of it shows. ` +
            `It adds no size of its own: give its parent a size (width/height, or flexGrow in a sized column), as on the web.`,
        )
        return
      }
      let scale = displayScale()
      apply({
        layout: { width: layout.width, height: layout.height },
        device: { width: viewport.width * scale, height: viewport.height * scale },
        scale,
      })
    })
  onLayout(run)
  createEffect(() => [displayScale(), opts?.deps?.()], run)
}
