// SQLite-backed HTTP response cache for the dev server's /__proxy__ endpoint.
//
// Project-local: stored at <cwd>/.srt-cache/cache.db. Opt-in via the --cache
// flag. Entries live forever; delete the .srt-cache directory to drop them.
//
// Cached: GET (and HEAD) 2xx responses with no Authorization on the request
// and no Cache-Control: no-store on either side. The cache key is
// sha256(method + "\n" + url); headers are intentionally not part of the key.

import { Database } from "bun:sqlite"
import { resolve, join } from "path"
import { mkdirSync } from "node:fs"
import { createHash } from "node:crypto"

const CACHE_DIR = ".srt-cache"
const CACHE_DB = "cache.db"

export type Decision = "hit" | "miss" | "bypass" | "skip"

export type Entry = {
  method: string
  url: string
  status: number
  headers: Record<string, string>
  body: Uint8Array
  cachedAt: number
}

let db: Database | null = null
let enabled = false

export function initCache(opts: { dir: string }) {
  mkdirSync(resolve(opts.dir, CACHE_DIR), { recursive: true })
  let d = new Database(join(resolve(opts.dir, CACHE_DIR), CACHE_DB), { create: true })
  d.run(`CREATE TABLE IF NOT EXISTS entries (
    key         TEXT PRIMARY KEY,
    method      TEXT NOT NULL,
    url         TEXT NOT NULL,
    status      INTEGER NOT NULL,
    headers     TEXT NOT NULL,
    body        BLOB NOT NULL,
    cached_at   INTEGER NOT NULL
  )`)
  db = d
  enabled = true
}

export function isEnabled(): boolean {
  return enabled
}

function keyFor(method: string, url: string): string {
  let h = createHash("sha256")
  h.update(method)
  h.update("\n")
  h.update(url)
  return h.digest("hex")
}

function cacheableMethod(method: string): boolean {
  return method === "GET" || method === "HEAD"
}

function hasNoStore(headerVal: string | null): boolean {
  if (!headerVal) return false
  return /(^|,)\s*no-store(\s*,|$)/i.test(headerVal)
}

function hasNoCache(headerVal: string | null): boolean {
  if (!headerVal) return false
  return /(^|,)\s*no-cache(\s*,|$)/i.test(headerVal)
}

export function shouldConsider(method: string, reqHeaders: Headers): { skip: boolean } {
  if (!enabled) return { skip: true }
  if (!cacheableMethod(method)) return { skip: true }
  if (reqHeaders.has("authorization")) return { skip: true }
  if (hasNoStore(reqHeaders.get("cache-control"))) return { skip: true }
  return { skip: false }
}

export function isBypass(reqHeaders: Headers): boolean {
  if (reqHeaders.get("x-srt-cache")?.toLowerCase() === "bypass") return true
  if (hasNoCache(reqHeaders.get("cache-control"))) return true
  return false
}

export function get(method: string, url: string): Entry | null {
  if (!db || !enabled) return null
  let row = db
    .query("SELECT method, url, status, headers, body, cached_at FROM entries WHERE key = ?")
    .get(keyFor(method, url)) as
    | {
        method: string
        url: string
        status: number
        headers: string
        body: Uint8Array
        cached_at: number
      }
    | null
  if (!row) return null
  return {
    method: row.method,
    url: row.url,
    status: row.status,
    headers: JSON.parse(row.headers),
    body: row.body,
    cachedAt: row.cached_at,
  }
}

export function put(
  method: string,
  url: string,
  status: number,
  headers: Record<string, string>,
  body: Uint8Array,
) {
  if (!db || !enabled) return
  if (status < 200 || status >= 300) return
  if (hasNoStore(headers["cache-control"] ?? null)) return
  db.run(
    `INSERT OR REPLACE INTO entries
     (key, method, url, status, headers, body, cached_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
    [keyFor(method, url), method, url, status, JSON.stringify(headers), body, Date.now()],
  )
}