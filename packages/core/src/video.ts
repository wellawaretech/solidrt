// Video playback, reactive (SolidJS) layer. There is no <video> element by
// design: a player streams into a texture id, so displaying it is just
// <texture src={video.texture()} /> (or <d-texture> for detached-layout
// placement), and every texture capability - shaders, offscreen targets,
// subtree effects - applies to video for free. A richer Video component
// composes on top of this in a higher layer, not in core.
//
// The imperative primitive lives in the `flux:video` module; import { open }
// from "flux:video" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import type { TextureId } from "flux:gpu"
import { open } from "flux:video"

export type VideoOptions = {
  /** Start playback as soon as the file is open. */
  autoplay?: boolean
}

/** An opened video as reactive accessors plus playback controls. */
export type VideoStream = {
  /** Texture id once the file is open, undefined while opening; render with <texture src={...}>. */
  texture(): TextureId | undefined
  /** Frame size, undefined while opening. */
  width(): number | undefined
  height(): number | undefined
  /** Duration in seconds, undefined while opening. */
  duration(): number | undefined
  /** Set if opening failed (unreadable file, unsupported codec). */
  error(): Error | undefined
  /** Start or resume playback (before open resolves, playback starts on resolve). */
  play(): void
  /** Pause playback; the current frame stays displayed. */
  pause(): void
  /** Whether playback is running (plain read, not a signal). */
  playing(): boolean
  /** Presentation time of the displayed frame in seconds (plain read, not a signal). */
  currentTime(): number
  /** Whether the last frame has been displayed (plain read, not a signal). */
  finished(): boolean
}

/**
 * Opens a video file (MP4, H.264 + AAC) and exposes it as reactive signals:
 * read texture() in JSX and the frames appear once playback starts. Closes
 * automatically when the reactive owner is disposed (e.g. the component
 * unmounts). For imperative use, call open() from "flux:video" directly.
 */
export function createVideo(path: string, options: VideoOptions = {}): VideoStream {
  let [texture, setTexture] = createSignal<TextureId | undefined>(undefined)
  let [width, setWidth] = createSignal<number | undefined>(undefined)
  let [height, setHeight] = createSignal<number | undefined>(undefined)
  let [duration, setDuration] = createSignal<number | undefined>(undefined)
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let player: Awaited<ReturnType<typeof open>> | undefined
  let disposed = false
  let wantPlay = options.autoplay ?? false

  open(path)
    .then((video) => {
      if (disposed) {
        video.close()
        return
      }
      player = video
      setTexture(video.texture)
      setWidth(video.width)
      setHeight(video.height)
      setDuration(video.duration)
      if (wantPlay) video.play()
    })
    .catch((e) => setError(e instanceof Error ? e : new Error(String(e))))

  onCleanup(() => {
    disposed = true
    if (player) {
      player.close()
      player = undefined
    }
  })

  return {
    texture,
    width,
    height,
    duration,
    error,
    play() {
      wantPlay = true
      player?.play()
    },
    pause() {
      wantPlay = false
      player?.pause()
    },
    playing: () => player?.playing() ?? false,
    currentTime: () => player?.currentTime() ?? 0,
    finished: () => player?.finished() ?? false,
  }
}
