// Speech recognition. A session captures the microphone, segments utterances
// by silence (Silero VAD) and transcribes each one with Whisper, delivering
// final transcripts through onResult. startRecognition resolves once the
// models are loaded and listening has begun; it rejects when loading fails.
// Models are passed as bytes so any source composes: flux:fs file(), fetch
// (incl. the dev-server file proxy), or a download cache layered on top.
// Requires a runtime built with speech support.

export type SpeechOptions = {
  /** A ggml Whisper model (file contents, e.g. ggml-tiny.en.bin). */
  model: Uint8Array
  /** A ggml Silero VAD model (file contents). */
  vadModel: Uint8Array
  /** Whisper language code; "auto" detects (multilingual models only). Default "en". */
  language?: string
  /** Explicit microphone device id from listMicrophones(). */
  microphone?: number
  /** Stop automatically after the first final result. */
  singleUtterance?: boolean
  /** Also deliver snapshot transcripts (final: false) while an utterance is still being spoken. */
  interimResults?: boolean
}

export type SpeechResult = {
  /** Transcript of the utterance (a snapshot of it when final is false). */
  text: string
  /** True for the completed utterance, false for interim snapshots. */
  final: boolean
}

export type SpeechSession = {
  /** Receive transcripts (replaces any previous callback). */
  onResult(callback: (result: SpeechResult) => void): void
  /** The user started speaking (replaces any previous callback). */
  onSpeechStart(callback: () => void): void
  /** The utterance ended; its final result follows once transcribed. */
  onSpeechEnd(callback: () => void): void
  /** Release the microphone and discard any utterance in progress. */
  stop(): void
}

export async function startRecognition(options: SpeechOptions): Promise<SpeechSession> {
  let started = await speech.start(options)
  return {
    onResult: (callback: (result: SpeechResult) => void) => speech.setResultCallback(started.handle, callback),
    onSpeechStart: (callback: () => void) => speech.setSpeechStartCallback(started.handle, callback),
    onSpeechEnd: (callback: () => void) => speech.setSpeechEndCallback(started.handle, callback),
    stop: () => speech.stop(started.handle),
  }
}