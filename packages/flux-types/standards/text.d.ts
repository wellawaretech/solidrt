// TextEncoder / TextDecoder. UTF-8 only (the only encoding the runtime needs).

interface TextEncoder {
  /** Always "utf-8". */
  readonly encoding: string
  /** Encode `input` (default "") to its UTF-8 bytes. */
  encode(input?: string): Uint8Array
}

declare let TextEncoder: {
  new (): TextEncoder
}

/** Options for the {@link TextDecoder} constructor. */
interface TextDecoderOptions {
  /** Throw on invalid UTF-8 instead of substituting U+FFFD. */
  fatal?: boolean
  /** Keep a leading byte-order mark instead of stripping it. */
  ignoreBOM?: boolean
}

/** Options for {@link TextDecoder.decode}. */
interface TextDecodeOptions {
  /** Hold an incomplete trailing UTF-8 sequence for the next call. */
  stream?: boolean
}

interface TextDecoder {
  /** Always "utf-8". */
  readonly encoding: string
  readonly fatal: boolean
  readonly ignoreBOM: boolean
  /**
   * Decode UTF-8 `input` (a Uint8Array or ArrayBuffer; default empty) to a
   * string. With `{ stream: true }`, an incomplete trailing sequence is held for
   * the next call.
   */
  decode(input?: Uint8Array | ArrayBuffer, options?: TextDecodeOptions): string
}

declare let TextDecoder: {
  /**
   * `label` must be a UTF-8 encoding label (the runtime is UTF-8 only); any other
   * label throws a RangeError.
   */
  new (label?: string, options?: TextDecoderOptions): TextDecoder
}