// APK Signature Scheme v2: the minimum Android accepts for targetSdk 30+ and
// exactly what the shipped solidrt-go.apk carries (one v2 block, no v1/v3).
// The APK Signing Block sits between the zip entries and the central
// directory: [size u64][id-value pairs][size u64]["APK Sig Block 42"], size
// counted from the pairs on. Digests cover three sections - entries, central
// directory, and an EOCD whose central-directory offset is rewritten to point
// at the signing block (verification strips the block, so the offset must not
// depend on it) - each split into 1 MB chunks: chunk digest
// sha256(0xa5, len u32, chunk), top digest sha256(0x5a, count u32, digests).

import { createHash, createPrivateKey, createPublicKey, sign } from "node:crypto"
import { buildEocd } from "./zip"
import { DEV_CERT_DER, DEV_KEY_PKCS8 } from "./key"

// v2 block ID inside the APK Signing Block.
const V2_BLOCK_ID = 0x7109871a
// Signature algorithm ID: RSASSA-PKCS1-v1_5 with SHA2-256.
const ALG_RSA_PKCS1_SHA256 = 0x0103
// Digest chunk size mandated by the scheme.
const CHUNK = 1024 * 1024

// Every v2 structure is a u32-length-prefixed blob; sequences are the
// concatenation of prefixed members, themselves prefixed as a whole.
function prefixed(body: Buffer): Buffer {
  let len = Buffer.alloc(4)
  len.writeUInt32LE(body.length, 0)
  return Buffer.concat([len, body])
}

function u32(value: number): Buffer {
  let out = Buffer.alloc(4)
  out.writeUInt32LE(value, 0)
  return out
}

function contentDigest(sections: Buffer[]): Buffer {
  let chunkDigests: Buffer[] = []
  for (let section of sections) {
    for (let off = 0; off < section.length; off += CHUNK) {
      let chunk = section.subarray(off, Math.min(off + CHUNK, section.length))
      chunkDigests.push(createHash("sha256").update(Buffer.from([0xa5])).update(u32(chunk.length)).update(chunk).digest())
    }
  }
  return createHash("sha256")
    .update(Buffer.from([0x5a]))
    .update(u32(chunkDigests.length))
    .update(Buffer.concat(chunkDigests))
    .digest()
}

// Sign the rebuilt zip and assemble the final APK:
// [entries][signing block][central directory][EOCD].
export function signApk(local: Buffer, cd: Buffer, entryCount: number): Buffer {
  let digest = contentDigest([local, cd, buildEocd(entryCount, cd.length, local.length)])

  let digests = prefixed(Buffer.concat([u32(ALG_RSA_PKCS1_SHA256), prefixed(digest)]))
  let signedData = Buffer.concat([
    prefixed(digests),
    prefixed(prefixed(DEV_CERT_DER)),
    prefixed(Buffer.alloc(0)), // additional attributes: none
  ])

  let key = createPrivateKey({ key: DEV_KEY_PKCS8, format: "der", type: "pkcs8" })
  let signature = prefixed(Buffer.concat([u32(ALG_RSA_PKCS1_SHA256), prefixed(sign("sha256", signedData, key))]))
  let publicKey = createPublicKey(key).export({ format: "der", type: "spki" })

  let signer = prefixed(Buffer.concat([prefixed(signedData), prefixed(signature), prefixed(publicKey)]))
  let v2Value = prefixed(signer)

  let pair = Buffer.concat([u32(V2_BLOCK_ID), v2Value])
  let pairLen = Buffer.alloc(8)
  pairLen.writeBigUInt64LE(BigInt(pair.length), 0)
  // Block size, stated twice, excludes the leading size field itself.
  let blockSize = Buffer.alloc(8)
  blockSize.writeBigUInt64LE(BigInt(8 + pair.length + 8 + 16), 0)
  let block = Buffer.concat([blockSize, pairLen, pair, blockSize, Buffer.from("APK Sig Block 42", "latin1")])

  return Buffer.concat([local, block, cd, buildEocd(entryCount, cd.length, local.length + block.length)])
}
