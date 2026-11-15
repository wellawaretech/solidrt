// The isolate of flux/tests/fetch-limits.test.ts: fetches from a second
// engine of the app, to show it shares the parent's per-host limits.

// Fire `n` GETs at `url` at once; resolves with how many completed.
export async function burst(url, n) {
  let all = []
  for (let i = 0; i < n; i++) all.push(fetch(url).then((res) => res.text()))
  return (await Promise.all(all)).length
}

// One POST to `url`, which is not retried after a 429; resolves with its status.
export async function post(url) {
  let res = await fetch(url, { method: "POST", body: "x" })
  await res.text()
  return res.status
}
