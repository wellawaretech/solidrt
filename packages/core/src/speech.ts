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
}

export type SpeechResult = {
  /** Transcript of one completed utterance. */
  text: string
}

export type SpeechSession = {
  /** Receive final transcripts (replaces any previous callback). */
  onResult(callback: (result: SpeechResult) => void): void
  /** Release the microphone and discard any utterance in progress. */
  stop(): void
}

export async function startRecognition(options: SpeechOptions): Promise<SpeechSession> {
  let started = await speech.start(options)
  return {
    onResult: (callback: (result: SpeechResult) => void) => speech.setResultCallback(started.handle, callback),
    stop: () => speech.stop(started.handle),
  }
}