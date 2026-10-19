// The engine verbs of an app test (lattice, dev client only), which
// @solidrt/test builds its surface on; tests import that, not this.
// The verbs work only in an engine a test host built (`sol test`) and throw
// anywhere else. App time there is frame / frameRate and nothing else: no
// frame runs unless one is asked for.
declare module "sol:test" {
  /**
   * Installs an RGBA8 image (`width` x `height`, tightly packed) as the
   * camera: every camera the app opens from now on shows it, approved at
   * once with no device, and an open one shows it from its next frame.
   * Throws on a size that does not match the pixels.
   */
  export function camera(width: number, height: number, pixels: Uint8Array): void
  /**
   * Installs a mono float clip at `sampleRate` Hz as the microphone: every
   * microphone the app opens at that rate from now on holds it from the
   * open, with no device, and an open one hears it now. Throws on a rate of
   * 0 or one that differs from an open test microphone's.
   */
  export function microphone(sampleRate: number, samples: Float32Array): void
  /**
   * Answers the next file dialog (`flux:dialog`) with `answer`, JSON text
   * of `{ files: [path | { path, name }, ...] }`, `{ directory: path }` or
   * `{ cancel: true }`: a call waiting on an answer takes it now, the next
   * call otherwise; no dialog is shown. Fulfills, once a call took it, with
   * that call's request as JSON text (its kind and options). Throws on an
   * answer of another shape.
   */
  export function dialog(answer: string): Promise<string>
  /**
   * Runs one frame: timers due by its time fire, then frame callbacks and
   * the flush, then the draw if anything demanded one. Fulfills once the
   * frame has run. One frame at a time: throws while the previous one is
   * still pending.
   */
  export function frame(): Promise<void>
  /** The frames per second this engine steps at (60 unless set). */
  export function frameRate(): number
  /** Another frame rate, a positive integer. Throws once a frame has run. */
  export function setFrameRate(fps: number): void
  /**
   * Another window, before the first frame: `width` x `height` logical
   * points (null: the size the run was started with) at display `scale`
   * (null: 1), so the window has `width * scale` by `height * scale`
   * pixels. Throws on a side or scale that is not positive, a product that
   * is not whole pixels, a side past the GPU's largest render target, and
   * once a frame has run. The resize lands ahead of the next frame;
   * `windowReady` waits for it.
   */
  export function setWindow(width: number | null, height: number | null, scale: number | null): void
  /** App time as of the last frame, in milliseconds; 0 before the first. */
  export function time(): number
  /**
   * Fulfills once the window's size has reached the engine, at once when
   * it already has. No frame runs for it and no time passes: a mounted
   * window builds its first frame on that size.
   */
  export function windowReady(): Promise<void>
  /**
   * Expands `events` (JSON text, the control API's `/input` event shape)
   * into the steps that are sent and returns what passes before each:
   * `[ms, frames]` per step, at least that much time and that many frames.
   * Throws on an invalid event, before anything is sent. The steps are then
   * sent with `inputStep`, in order.
   */
  export function inputPlan(events: string): [ms: number, frames: number][]
  /**
   * Sends step `index` of the current plan into the input pipeline: a
   * down, an up, a key or a wheel is dispatched on arrival, ahead of the
   * next frame; a move with that frame.
   */
  export function inputStep(index: number): void
  /**
   * Runs frames until the app is at rest: nothing in flight, no timer due,
   * no frame demanded. Work in flight is waited for with no app time
   * passing. Rejects once `maxMs` of app time have passed without rest,
   * with what is left in the message.
   */
  export function settle(maxMs: number): Promise<void>
  /**
   * Delivers a link the way an OS-routed one arrives (the raw string, on
   * the `link` event, ahead of the next frame). Returns whether anything
   * listens for links.
   */
  export function link(link: string): boolean
  /**
   * Calls a debug command the app registered with `registerDebug`. `args`
   * and the result are JSON text (null for no args). Throws on an unknown
   * name, a throwing command and a result JSON cannot carry.
   */
  export function debug(name: string, args: string | null): string
  /** The performance statistics as `/stats` reports them, JSON text; the
   * window in ms (null = default) or in frames (wins when given). */
  export function stats(windowMs: number | null, windowFrames: number | null): string
  /**
   * The pixels a node's subtree paints: RGBA8, premultiplied, rows top to
   * bottom, at the display scale. Drawn at once from the tree as it is; no
   * frame runs and no app time passes.
   */
  export function capture(node: number): { width: number; height: number; data: Uint8Array }
  /**
   * Run one frame with a capture of `node` queued ahead of it, so the
   * frame's own paint services it: fulfills, once the frame has run, with
   * what that frame drew of the node (the shape of `capture`). Written
   * and painted in one frame, where `capture` paints the tree afresh
   * after the fact. One frame at a time, as `frame`.
   */
  export function painted(node: number): Promise<{ width: number; height: number; data: Uint8Array }>
}
