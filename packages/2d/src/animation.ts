// Sprite frame animation: a clip (frames + fps) with a shared clock that
// steps every attached sprite's frame through setSprite. One frame callback
// per playing clip, registered without demand, and one frame write per
// attached sprite per STEP (not per display frame), so an 8fps walk cycle
// generates 8 publishes and 8 presents a second and a paused clip costs
// nothing - the demand-gate story unchanged. The clock is the app's frame
// time: the clip freezes and resumes with the dev tools' clock, plays at
// its own speed under `sol render`, and a step lands on the frame it is
// due in. The clip
// does not own its sprites (destroySprite prunes lazily on the next step),
// and a sprite belongs to at most one animation. Plain JS over setSprite,
// so it works on both layer kinds and composes with <Sprite> via ref (leave
// the frame prop off - the clip owns that field).
import { getOwner, onCleanup, onFrame, runWithOwner } from "@solidrt/core"
import type { Frame } from "./frames.ts"
import type { Sprite } from "./layer.ts"
import { setSprite } from "./layer.ts"

export type AnimationOptions = {
  /** Wrap around (default) or play once and hold the last frame. */
  loop?: boolean
  /** Skip the owner-scoped auto-dispose (the core resource contract). */
  autoFree?: boolean
}

export type SpriteAnimation = {
  /** Current frame index into the clip. */
  frame(): number
  /** True while the clock advances: born playing, false after pause() and
   * after a one-shot ends. */
  playing(): boolean
  /**
   * Attach a sprite: it shows the clip's current frame immediately and
   * steps with the shared clock (every attached sprite shows the SAME
   * index). A sprite belongs to at most one animation - attaching detaches
   * it from its previous one.
   */
  add(sprite: Sprite): void
  /** Detach; the sprite keeps the frame it is showing. */
  remove(sprite: Sprite): void
  /** Resume a paused clip (phase kept), or replay a finished one-shot
   * from frame 0. */
  play(): void
  /** Freeze the clock; attached sprites hold the current frame. */
  pause(): void
  /** One-shot clips (loop: false): the last frame finished its display
   * time. Fires once per play-through, never on pause or dispose. */
  onFinish?: () => void
  /** Stop the clock and detach every sprite. Owner-scoped like a layer
   * (opt out with autoFree: false). */
  dispose(): void
}

// A sprite's current animation - add() moves the sprite here, so two clips
// never fight over one sprite's frame field.
let attachedTo = new WeakMap<Sprite, SpriteAnimation>()

/**
 * A frame animation clip: `frames` (usually a slice of a `grid()` result)
 * played at `fps`. Sprites attach with `add`; the clip steps their `frame`
 * on the app's frame time, independent of display rate.
 */
export function createAnimation(frames: Frame[], fps: number, opts?: AnimationOptions): SpriteAnimation {
  if (frames.length === 0) throw new Error("createAnimation: frames is empty")
  if (!(Number.isFinite(fps) && fps > 0)) {
    throw new Error(`createAnimation: fps must be positive, got ${fps}`)
  }
  let loop = opts?.loop !== false
  let sprites = new Set<Sprite>()
  let current = 0
  let playing = true
  let ended = false
  let disposed = false
  // The running clock's frame callback, null while stopped.
  let stop: (() => void) | null = null
  let elapsed = 0 // ms of clip time played so far
  // The frame time the clip last advanced to, null until the running
  // clock's first frame: that one only anchors, so a clip starts at the
  // frame it was started in, whenever in it.
  let lastTick: number | null = null

  function startClock(): void {
    if (stop !== null || !playing || sprites.size === 0) return
    // A looping single-frame clip never changes; a one-shot still needs
    // the clock once, to end.
    if (loop && frames.length === 1) return
    lastTick = null
    // No demand: the clip follows the frames the app runs anyway and asks
    // for one only by writing a step. Under no owner: the clock is the
    // clip's (stopClock ends it), not the scope's that attached a sprite.
    stop = runWithOwner(null, () => onFrame(tick, { demand: false }))
  }

  function stopClock(): void {
    if (stop === null) return
    stop()
    stop = null
  }

  function writeAll(): void {
    let frame = frames[current]!
    for (let sprite of sprites) {
      if (sprite.layer === null) {
        // Removed sprite: prune (deleting during for..of is safe on a Set).
        sprites.delete(sprite)
        attachedTo.delete(sprite)
      } else setSprite(sprite, { frame })
    }
    if (sprites.size === 0) stopClock()
  }

  function setIndex(idx: number): void {
    if (idx === current) return
    current = idx
    writeAll()
  }

  function tick(now: number): void {
    // Never backward: across a hot reload the frame time restarts.
    if (lastTick !== null) elapsed += Math.max(0, now - lastTick)
    lastTick = now
    let raw = Math.floor((elapsed * fps) / 1000)
    if (!loop && raw >= frames.length) {
      // The last frame has shown for its full duration: hold it and stop.
      setIndex(frames.length - 1)
      stopClock()
      playing = false
      ended = true
      handle.onFinish?.()
      return
    }
    setIndex(loop ? raw % frames.length : Math.min(raw, frames.length - 1))
  }

  let handle: SpriteAnimation = {
    frame: () => current,
    playing: () => playing,
    onFinish: undefined,
    add(sprite) {
      if (disposed) throw new Error("createAnimation: add on a disposed animation")
      attachedTo.get(sprite)?.remove(sprite)
      attachedTo.set(sprite, handle)
      sprites.add(sprite)
      setSprite(sprite, { frame: frames[current]! })
      startClock()
    },
    remove(sprite) {
      if (!sprites.delete(sprite)) return
      attachedTo.delete(sprite)
      if (sprites.size === 0) stopClock()
    },
    play() {
      if (disposed || playing) return
      if (ended) {
        ended = false
        elapsed = 0
        current = 0
        writeAll()
      }
      playing = true
      startClock()
    },
    pause() {
      if (!playing) return
      stopClock()
      playing = false
    },
    dispose() {
      if (disposed) return
      stopClock()
      playing = false
      disposed = true
      for (let sprite of sprites) attachedTo.delete(sprite)
      sprites.clear()
    },
  }

  if (opts?.autoFree !== false && getOwner()) onCleanup(() => handle.dispose())
  return handle
}
