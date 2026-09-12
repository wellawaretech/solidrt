// Video playback, reactive (SolidJS) layer. There is no <video> element by
// design: a player streams into a texture id, so displaying it is just
// <texture src={video.texture()} /> (or <d-texture> for detached-layout
// placement), and every texture capability - shaders, offscreen targets,
// subtree effects - applies to video for free. A richer Video component
// composes on top of this in a higher layer, not in core.
//
// A plane player (`present: "plane"`, Android) has no texture: the platform
// composites the picture fullscreen beneath the UI on the video's own clock,
// and the UI simply draws over it. It exists from open until the owner is
// disposed. On a platform without a plane the open fails (error()), and an
// app that runs everywhere falls back to a texture player itself.
//
// The imperative primitive lives in the `flux:video` module; import { open }
// from "flux:video" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import type { TextureId } from "flux:gpu"
import { open, type VideoPlane, type VideoPlayer } from "flux:video"

export type VideoOptions = {
  /** Start playback as soon as the file is open. */
  autoplay?: boolean
  /**
   * Where the picture goes: a texture (default, displayed by the app) or the
   * platform's video plane (fullscreen beneath the UI, Android only).
   */
  present?: "texture" | "plane"
  /** Plane only: letterboxed (default) or cropped to fill the window. */
  fit?: "contain" | "cover"
}

/** An opened video as reactive accessors plus playback controls. */
export type VideoStream = {
  /** True once the file is open (and, for a plane, the plane exists). */
  ready(): boolean
  /** Texture id once open, undefined while opening and always for a plane player; render with <texture src={...}>. */
  texture(): TextureId | undefined
  /** Frame size, undefined while opening. */
  width(): number | undefined
  height(): number | undefined
  /** Duration in seconds, undefined while opening or when the source has none. */
  duration(): number | undefined
  /** Set if opening failed (unreadable file, unsupported codec, no plane on this platform). */
  error(): Error | undefined
  /** Start or resume playback (before open resolves, playback starts on resolve). */
  play(): void
  /** Pause playback; the current frame stays displayed. */
  pause(): void
  /**
   * Seek to a time in seconds (plane players; a texture player ignores it
   * until it gains transport). While paused the target frame is shown.
   */
  seek(seconds: number): void
  /** Whether playback is running (plain read, not a signal). */
  playing(): boolean
  /** Presentation time of the displayed frame in seconds (plain read, not a signal). */
  currentTime(): number
  /** Whether the last frame has been displayed (plain read, not a signal). */
  finished(): boolean
}

type AnyPlayer = VideoPlayer | VideoPlane

/**
 * Opens a video file (MP4, VP9 + AAC) and exposes it as reactive signals:
 * read texture() in JSX and the frames appear once playback starts. Closes
 * automatically when the reactive owner is disposed (e.g. the component
 * unmounts). For imperative use, call open() from "flux:video" directly.
 */
export function createVideo(path: string, options: VideoOptions = {}): VideoStream {
  let [ready, setReady] = createSignal(false)
  let [texture, setTexture] = createSignal<TextureId | undefined>(undefined)
  let [width, setWidth] = createSignal<number | undefined>(undefined)
  let [height, setHeight] = createSignal<number | undefined>(undefined)
  let [duration, setDuration] = createSignal<number | undefined>(undefined)
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let player: AnyPlayer | undefined
  let disposed = false
  let wantPlay = options.autoplay ?? false

  let opened: Promise<AnyPlayer> =
    options.present === "plane" ? open(path, { present: "plane", fit: options.fit }) : open(path)
  opened
    .then((video) => {
      if (disposed) {
        video.close()
        return
      }
      player = video
      if ("texture" in video) setTexture(video.texture)
      setWidth(video.width)
      setHeight(video.height)
      setDuration(video.duration)
      setReady(true)
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
    ready,
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
    seek(seconds: number) {
      if (player && "seek" in player) player.seek(seconds)
    },
    playing: () => player?.playing() ?? false,
    currentTime: () => player?.currentTime() ?? 0,
    finished: () => player?.finished() ?? false,
  }
}
