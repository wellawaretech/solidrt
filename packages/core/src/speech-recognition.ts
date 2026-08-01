// Speech recognition. A session captures the microphone, segments utterances
// by silence (Silero VAD) and transcribes each one with Whisper, delivering
// final transcripts through onResult. With wakeWord the session starts
// asleep behind an efficient wake word detector (livekit-wakeword) and only
// transcribes after the wake word. startRecognition resolves once the models
// are loaded and listening has begun; it rejects when the microphone cannot be
// opened or the models fail to load.
// Models are passed as bytes so any source composes: flux:fs file(), fetch
// (incl. the dev-server file proxy), or a download cache layered on top.
// Requires a runtime built with speech support.

import { createSignal, onCleanup } from "@solidjs/signals"

export type SpeechOptions = {
  /** A ggml Whisper model (file contents, e.g. ggml-tiny.en.bin). */
  model: Uint8Array
  /** A ggml Silero VAD model (file contents). */
  vadModel: Uint8Array
  /** Whisper language code; "auto" detects (multilingual models only). Default "en". */
  lang?: string
  /** Explicit microphone device id from listMicrophones(). */
  microphone?: number
  /**
   * Keep transcribing utterance after utterance. Default true. Set false to
   * stop after the first final result (with wakeWord: re-arm instead, one
   * result per wake). Inverse of the Web Speech API default (false there).
   */
  continuous?: boolean
  /** Also deliver snapshot transcripts (final: false) while an utterance is still being spoken. */
  interimResults?: boolean
  /**
   * Wake word: start asleep, fire onWake when it is heard, then transcribe
   * the speech that follows. How the wake word is specified depends on the
   * engine. The current engine detects with a trained classifier and takes
   * the model's bytes (livekit-wakeword ONNX, e.g. the pretrained "hey
   * livekit"; custom phrases are trained offline with its toolkit). Phrase
   * strings are reserved for engines that match text; passing them to this
   * engine rejects with an error.
   */
  wakeWord?: Uint8Array | string | string[]
  /** Detector confidence (0..1) that counts as a wake. Default 0.5. */
  wakeThreshold?: number
}

export type SpeechResult = {
  /** Transcript of the utterance (a snapshot of it when isFinal is false). */
  transcript: string
  /** True for the completed utterance, false for interim snapshots. */
  isFinal: boolean
}

export type SpeechSession = {
  /** Receive transcripts (replaces any previous callback). */
  onResult(callback: (result: SpeechResult) => void): void
  /** The user started speaking (replaces any previous callback). */
  onSpeechStart(callback: () => void): void
  /** The utterance ended; its final result follows once transcribed. */
  onSpeechEnd(callback: () => void): void
  /** The wake word was heard (wakeWordModel sessions only). */
  onWake(callback: () => void): void
  /** Release the microphone and discard any utterance in progress. */
  stop(): void
}

export async function startRecognition(options: SpeechOptions): Promise<SpeechSession> {
  let started = await speech.start(options)
  return {
    onResult: (callback: (result: SpeechResult) => void) => speech.setResultCallback(started.handle, callback),
    onSpeechStart: (callback: () => void) => speech.setSpeechStartCallback(started.handle, callback),
    onSpeechEnd: (callback: () => void) => speech.setSpeechEndCallback(started.handle, callback),
    onWake: (callback: () => void) => speech.setWakeCallback(started.handle, callback),
    stop: () => speech.stop(started.handle),
  }
}

/** A live recognition session as reactive accessors. */
export type SpeechStream = {
  /** True once the models are loaded and listening has begun. */
  ready(): boolean
  /** Latest transcript, "" until the first result (a snapshot while isFinal is false). */
  transcript(): string
  /** Whether transcript() is the completed utterance rather than an interim snapshot. */
  isFinal(): boolean
  /** True while the user is mid-utterance (between speech start and end). */
  speaking(): boolean
  /** True from when an utterance ends until its transcript arrives. */
  transcribing(): boolean
  /**
   * wakeWord sessions only: true from when the wake word is heard until the
   * following command is transcribed (the next final result), then false.
   * Always false without a wakeWord.
   */
  awake(): boolean
  /** Set if loading or starting failed. */
  error(): Error | undefined
}

/**
 * Starts speech recognition and exposes it as reactive signals: read
 * transcript()/isFinal() for results, speaking() and awake() for session
 * state. Stops when the reactive owner is disposed. The lower-level
 * startRecognition() is the imperative alternative.
 */
export function createSpeechRecognition(options: SpeechOptions): SpeechStream {
  let [ready, setReady] = createSignal(false)
  let [transcript, setTranscript] = createSignal("")
  let [isFinal, setIsFinal] = createSignal(false)
  let [speaking, setSpeaking] = createSignal(false)
  let [transcribing, setTranscribing] = createSignal(false)
  let [awake, setAwake] = createSignal(false)
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let handle: number | undefined
  let disposed = false

  speech
    .start(options)
    .then((started) => {
      if (disposed) {
        speech.stop(started.handle)
        return
      }
      handle = started.handle
      speech.setResultCallback(started.handle, (result) => {
        setTranscript(result.transcript)
        setIsFinal(result.isFinal)
        if (result.isFinal) {
          setTranscribing(false)
          setAwake(false)
        }
      })
      speech.setSpeechStartCallback(started.handle, () => setSpeaking(true))
      speech.setSpeechEndCallback(started.handle, () => {
        setSpeaking(false)
        setTranscribing(true)
      })
      speech.setWakeCallback(started.handle, () => setAwake(true))
      setReady(true)
    })
    .catch((e) => setError(e instanceof Error ? e : new Error(String(e))))

  onCleanup(() => {
    disposed = true
    if (handle !== undefined) {
      speech.stop(handle)
      handle = undefined
    }
  })

  return { ready, transcript, isFinal, speaking, transcribing, awake, error }
}