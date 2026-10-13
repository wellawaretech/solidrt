// The pure fill-size math: arithmetic on plain numbers, no GUI reads, so
// the headless check (tests/fill-math.test.ts) drives it directly - the
// same split as @solidrt/2d's oversample-math.ts. components/fill.ts
// binds it to the followed leaf.

/** The density knobs a `<Scene>` and a `<View3d>` take: texels per
 * on-screen pixel, as a scale and a budget. The 2d packages' DensityProps
 * are the same knobs with an integer oversample. */
export type DensityProps = {
  /**
   * Render scale over the tracked device-pixel size (default 1): 2 renders
   * four times the pixels and the leaf's linear sampler box-averages them
   * down (supersampling, cheap anti-aliasing beyond `samples`), 0.5 renders
   * a quarter (a heavy scene on a weak GPU). Unity's render scale, Godot's
   * `scaling_3d_scale`, r3f's `dpr`. The leaf's layout, pointer events
   * and the camera controls are unaffected: they scale from the layout
   * box, not the target. Reactive. Fill mode only - with `width`/`height`
   * the target IS the pixels, and the prop throws.
   */
  renderScale?: number
  /**
   * Cap on the target's pixel count: when the tracked size (after
   * `renderScale`) exceeds it, both axes scale down by the same factor,
   * so the aspect holds. One cost across devices, which a scale alone
   * cannot give (a 0.7 that suits a laptop panel is wasted on a phone and
   * too much on a 4K screen); fill-rate-bound scenes on an iGPU under a
   * HiDPI panel halve their main pass at ~2 Mpx with no visible loss under
   * a HUD. Absent, native density: the runtime picks no budget, only the
   * app knows its scene's cost. Reactive. Fill mode only, like
   * `renderScale`.
   */
  maxPixels?: number
}

/** The size the decision computes: the target's, and the leaf's in device pixels. */
export type FillSize = { width: number; height: number; deviceWidth: number; deviceHeight: number }

/** Dev validation of the knobs, naming the component: finite and positive, else throw. */
export function checkDensityProps(what: string, props: DensityProps): void {
  let { renderScale, maxPixels } = props
  if (renderScale !== undefined && !(Number.isFinite(renderScale) && renderScale > 0)) {
    throw new Error(`${what}: renderScale must be a positive number, got ${renderScale}`)
  }
  if (maxPixels !== undefined && !(Number.isFinite(maxPixels) && maxPixels > 0)) {
    throw new Error(`${what}: maxPixels must be a positive number, got ${maxPixels}`)
  }
}

/**
 * The size decision: the leaf's box in device pixels, scaled by
 * `renderScale`, held under `maxPixels` by one factor on both axes (so
 * the aspect holds), each axis rounded and at least one pixel. The
 * rounding may leave the product a row or column over the budget: it is
 * a budget, not a hard cap.
 */
export function fillSize(deviceWidth: number, deviceHeight: number, props: DensityProps): FillSize {
  let scale = props.renderScale ?? 1
  let w = deviceWidth * scale
  let h = deviceHeight * scale
  let pixels = w * h
  if (props.maxPixels !== undefined && pixels > props.maxPixels) {
    let k = Math.sqrt(props.maxPixels / pixels)
    w *= k
    h *= k
  }
  return {
    width: Math.max(1, Math.round(w)),
    height: Math.max(1, Math.round(h)),
    deviceWidth: Math.max(1, Math.round(deviceWidth)),
    deviceHeight: Math.max(1, Math.round(deviceHeight)),
  }
}
