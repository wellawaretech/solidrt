// crypto. The Web Crypto surface flux provides: `subtle.digest` only. No key
// material, no encryption, no random: an app hashes bytes (content
// addressing, integrity checks) and the rest of the standard waits for a need.

interface SubtleCrypto {
  /**
   * Hash `data` (a Uint8Array or ArrayBuffer) with `algorithm`, one of
   * "SHA-256", "SHA-384", "SHA-512" (as a string or `{ name }`). Resolves to
   * the digest as an ArrayBuffer. Other algorithms (SHA-1 included) reject.
   */
  digest(algorithm: string | { name: string }, data: Uint8Array | ArrayBuffer): Promise<ArrayBuffer>
}

interface Crypto {
  readonly subtle: SubtleCrypto
}

declare var crypto: Crypto
