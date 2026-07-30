// atob / btoa. Base64 over "binary strings" (WHATWG): the string is a byte
// container, not text - each char code is one raw byte in 0..=255, with no
// UTF-8 step in either direction.

/**
 * Base64-encode a binary string: each char code is taken as one raw byte.
 * Throws if the string contains a code point above 255 (encode real text by
 * taking its bytes first, e.g. via {@link TextEncoder}).
 */
declare function btoa(data: string): string
/**
 * Base64-decode to a binary string: each decoded byte becomes one char code
 * (read the bytes back with `charCodeAt`). ASCII whitespace in the input is
 * ignored; anything else that is not valid base64 throws.
 */
declare function atob(data: string): string
