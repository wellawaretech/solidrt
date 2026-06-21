// Microphone capture globals (gui-enabled runtime only). Bare globals, not a
// `flux:*` module; helper types stay module-scoped via the trailing `export {}`.

/** A microphone device from {@link microphone.listMicrophones}. */
type MicrophoneDevice = {
  /** Device id to pass as `open({ microphone })`. */
  id: number
  /** Human-readable device name. */
  name: string
}

/** Options for {@link microphone.open}. */
type MicrophoneOpenOptions = {
  /** Device id from {@link MicrophoneDevice}. Omit to use the default. */
  microphone?: number
  /** Capture sample rate in Hz. Defaults to 16000. */
  sampleRate?: number
}

/** An opened microphone session. */
type MicrophoneSession = {
  /** Session handle for `read` / `close`. */
  handle: number
  /** The actual capture sample rate in Hz. */
  sampleRate: number
}

declare global {
  /**
   * Microphone capture. The lower-level primitive that `@solidrt/core`'s
   * `createMicrophone` wraps. Available only on a gui-enabled runtime.
   */
  const microphone: {
    /** List the available microphone devices. */
    listMicrophones(): MicrophoneDevice[]
    /**
     * Open a microphone. Synchronous: current platforms expose no audio
     * permission prompt.
     */
    open(options?: MicrophoneOpenOptions): MicrophoneSession
    /** Drain the mono float samples captured since the last read. */
    read(handle: number): Float32Array
    /** Close a session by its `handle`. */
    close(handle: number): void
  }
}

export {}