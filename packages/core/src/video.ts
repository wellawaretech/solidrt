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
// disposed, and it streams: the source may be a URL, read as it plays. On a
// platform without a plane the open fails (error() with kind "no-plane"),
// and an app that runs everywhere falls back to a texture player itself.
//
// The imperative primitive lives in the `flux:video` module; import { open }
// from "flux:video" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import type { TextureId } from "flux:gpu"
import { open, type VideoError, type VideoPlane, type VideoPlayer, type VideoSource } from "flux:video"

export type { VideoError, VideoErrorKind, VideoSource } from "flux:video"

export type VideoOptions = {
  /** Start playback as soon as the source is open. */
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
  /** True once the source is open (and, for a plane, the plane exists). */
  ready(): boolean
  /** Texture id once open, undefined while opening and always for a plane player; render with <texture src={...}>. */
  texture(): TextureId | undefined
  /** Frame size, undefined while opening. */
  width(): number | undefined
  height(): number | undefined
  /** Duration in seconds, undefined while opening or when the source has no known end. */
  duration(): number | undefined
  /**
   * The failure: opening failed (unreadable or unreachable source,
   * unsupported codec, no plane on this platform), or playback stopped on
   * one later. Its `kind` says which; an app's plane-to-texture fallback
   * keys on "no-plane" only.
   */
  error(): VideoError | undefined
  /** Start or resume playback (before open resolves, playback starts on resolve). */
  play(): void
  /** Pause playback; the current frame stays displayed. */
  pause(): void
  /**
   * Seek to a time in seconds. A no-op while opening, on a texture player,
   * and on a source that cannot seek (see `seekable`). While paused the
   * target frame is shown.
   */
  seek(seconds: number): void
  /** Whether `seek` does anything (plain read, not a signal). */
  seekable(): boolean
  /** Whether playback is running (plain read, not a signal). */
  playing(): boolean
  /** Presentation time of the displayed frame in seconds (plain read, not a signal). */
  currentTime(): number
  /** Whether the last frame has been displayed (plain read, not a signal). */
  finished(): boolean
  /** Whether playback is held for the source to catch up (plain read, not a signal). */
  buffering(): boolean
}

type AnyPlayer = VideoPlayer | VideoPlane

function toVideoError(e: unknown): VideoError {
  if (e instanceof Error && "kind" in e) return e as VideoError
  let error = new Error(String(e)) as VideoError
  error.kind = "decode"
  return error
}

/**
 * Opens a video (WebM, VP9 + Opus) from a path, an http(s) URL (plane
 * players only) or a file(), and exposes it as reactive signals: read
 * texture() in JSX and the frames appear once playback starts. Closes
 * automatically when the reactive owner is disposed (e.g. the component
 * unmounts), abandoning an open still in flight. For imperative use, call
 * open() from "flux:video" directly.
 */
export function createVideo(source: VideoSource, options: VideoOptions = {}): VideoStream {
  let [ready, setReady] = createSignal(false)
  let [texture, setTexture] = createSignal<TextureId | undefined>(undefined)
  let [width, setWidth] = createSignal<number | undefined>(undefined)
  let [height, setHeight] = createSignal<number | undefined>(undefined)
  let [duration, setDuration] = createSignal<number | undefined>(undefined)
  let [error, setError] = createSignal<VideoError | undefined>(undefined)
  let player: AnyPlayer | undefined
  let disposed = false
  let wantPlay = options.autoplay ?? false
  // Disposal aborts a pending open, so a player never appears after the
  // owner is gone.
  let abort = new AbortController()

  let opened: Promise<AnyPlayer> =
    options.present === "plane"
      ? open(source, { present: "plane", fit: options.fit, signal: abort.signal })
      : open(source, { signal: abort.signal })
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
      // A failure that stops playback later lands in the same signal.
      video.failed.then((failure) => {
        if (failure && !disposed) setError(failure)
      })
    })
    .catch((e) => {
      if (!disposed) setError(toVideoError(e))
    })

  onCleanup(() => {
    disposed = true
    abort.abort()
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
      player?.seek(seconds)
    },
    seekable: () => player?.seekable ?? false,
    playing: () => player?.playing() ?? false,
    currentTime: () => player?.currentTime() ?? 0,
    finished: () => player?.finished() ?? false,
    buffering: () => player?.buffering() ?? false,
  }
}
