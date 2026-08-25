import * as cache from "./cache"

// The /__proxy__ endpoint: forward a request to the URL in X-SRT-Proxy-Url,
// buffering the upstream response, with the opt-in sqlite cache in front.

function headersToObject(h: Headers): Record<string, string> {
  let out: Record<string, string> = {}
  h.forEach((v, k) => {
    out[k] = v
  })
  return out
}

export async function handleProxy(req: Request): Promise<Response> {
  let target = req.headers.get("x-srt-proxy-url")
  if (!target) {
    return new Response("Missing X-SRT-Proxy-Url", { status: 400 })
  }

  let forwardHeaders = new Headers(req.headers)
  forwardHeaders.delete("host")
  forwardHeaders.delete("x-srt-proxy-url")
  forwardHeaders.delete("x-srt-cache")
  forwardHeaders.delete("content-length")

  let cacheStatus: cache.Decision = "skip"
  let cacheable = !cache.shouldConsider(req.method, req.headers).skip
  let bypass = cacheable && cache.isBypass(req.headers)

  if (cacheable && !bypass) {
    let hit = await cache.get(req.method, target)
    if (hit) {
      console.log(`[cli] proxy ${req.method} ${target} [cache hit]`)
      let respHeaders = new Headers(hit.headers)
      respHeaders.set("x-srt-cache", "hit")
      return new Response(hit.body, { status: hit.status, headers: respHeaders })
    }
  }

  let hasBody = req.method !== "GET" && req.method !== "HEAD"
  if (cacheable) {
    cacheStatus = bypass ? "bypass" : "miss"
    console.log(`[cli] proxy ${req.method} ${target} [${cacheStatus}]`)
  } else {
    console.log(`[cli] proxy ${req.method} ${target}`)
  }

  try {
    let upstream = await fetch(target, {
      method: req.method,
      headers: forwardHeaders,
      body: hasBody ? await req.bytes() : undefined,
    })
    let respHeaders = new Headers(upstream.headers)
    respHeaders.delete("content-encoding")
    respHeaders.delete("transfer-encoding")

    let bodyBytes = await upstream.bytes()
    if (cacheable) {
      await cache.put(req.method, target, upstream.status, headersToObject(respHeaders), bodyBytes)
      respHeaders.set("x-srt-cache", cacheStatus)
    }
    return new Response(bodyBytes, {
      status: upstream.status,
      statusText: upstream.statusText,
      headers: respHeaders,
    })
  } catch (e) {
    console.log(`[cli] proxy error ${target}: ${String(e)}`)
    return new Response(`Proxy error: ${String(e)}`, { status: 502 })
  }
}
