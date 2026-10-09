// flux:p2p: two local endpoints on this machine (no relay, no discovery),
// one stream between them, and the readable/writable pair over it: the echo,
// a write after the peer went away, and an abort the peer's read reports.
import { expect, test } from "flux:test"
import { Endpoint, type P2pStream } from "flux:p2p"

const PROTOCOL = "flux-test/1"
// A write on a connection the peer closed fails once the close has been
// processed here; the retries bound how long that may take.
const GONE_RETRIES = 50
const GONE_RETRY_MS = 100

let sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

// Two endpoints and a stream between them: `a` dialled and wrote the
// greeting, `b` accepted and read it. The dialer writes first because a
// QUIC stream reaches the acceptor with its first bytes.
const GREETING = "hello"

async function pair() {
  let server = await Endpoint.create({ local: true, protocols: [PROTOCOL] })
  let client = await Endpoint.create({ local: true })
  let accepted = server.accept(PROTOCOL)[Symbol.asyncIterator]().next()
  let a = await client.connect(await server.ticket(), PROTOCOL)
  let writer = a.writable.getWriter()
  await writer.write(GREETING)
  writer.releaseLock()
  let b = (await accepted).value as P2pStream
  let reader = b.readable.getReader()
  expect(new TextDecoder().decode((await reader.read()).value)).toBe(GREETING)
  reader.releaseLock()
  return { server, client, a, b }
}

test("a stream pair echoes over readable and writable", async () => {
  let { server, client, a, b } = await pair()
  expect(b.remoteId).toBe(client.id)
  let writer = a.writable.getWriter()
  await writer.write("ping")
  await writer.close()
  // The half-close arrives: b reads to end-of-stream, then answers.
  expect(await new Response(b.readable).text()).toBe("ping")
  let back = b.writable.getWriter()
  await back.write("pong")
  await back.close()
  expect(await new Response(a.readable).text()).toBe("pong")
  a.close()
  b.close()
  await client.close()
  await server.close()
})

test("a write after the peer went away rejects", async () => {
  let { server, client, a, b } = await pair()
  b.close()
  await server.close()
  let writer = a.writable.getWriter()
  let failed = false
  for (let i = 0; i < GONE_RETRIES && !failed; i++) {
    try {
      await writer.write("still there?")
      await sleep(GONE_RETRY_MS)
    } catch {
      failed = true
    }
  }
  expect(failed).toBe(true)
  a.close()
  await client.close()
})

test("aborting the writable resets the stream for the peer and errors this side", async () => {
  let { server, client, a, b } = await pair()
  let peerRead = b.readable.getReader().read()
  await a.writable.abort(new Error("gave up"))
  await expect(peerRead).rejects.toThrow("reset")
  await expect(a.readable.getReader().read()).rejects.toThrow("gave up")
  b.close()
  await client.close()
  await server.close()
})
