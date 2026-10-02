// crypto.subtle.digest: the three SHA-2 digests over bytes.
import { expect, test } from "flux:test"

let hex = (buf: ArrayBuffer) =>
  Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")

test("subtle.digest computes SHA-256, SHA-384 and SHA-512", async () => {
  let bytes = new TextEncoder().encode("abc")
  let d256 = await crypto.subtle.digest("SHA-256", bytes)
  expect(d256 instanceof ArrayBuffer).toBe(true)
  expect(hex(d256)).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
  // An ArrayBuffer input and the { name } spelling are accepted too.
  let d512 = await crypto.subtle.digest({ name: "SHA-512" }, bytes.buffer as ArrayBuffer)
  expect(d512.byteLength).toBe(64)
  expect(hex(d512).slice(0, 16)).toBe("ddaf35a193617aba")
  let d384 = await crypto.subtle.digest("sha-384", new Uint8Array(0))
  expect(d384.byteLength).toBe(48)
})

test("subtle.digest rejects an unsupported algorithm and a non-byte input", async () => {
  await expect(crypto.subtle.digest("SHA-1", new Uint8Array(0))).rejects.toThrow(
    /^crypto.subtle.digest: unsupported algorithm "SHA-1" \(SHA-256, SHA-384, SHA-512\)$/,
  )
  // @ts-expect-error a string is not bytes; the call has to say so at runtime too
  await expect(crypto.subtle.digest("SHA-256", "abc")).rejects.toThrow(/^crypto.subtle.digest: data must be a Uint8Array or ArrayBuffer$/)
})
