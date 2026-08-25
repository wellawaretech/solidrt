// SQLite-backed HTTP response cache for the dev server's /__proxy__ endpoint.
//
// Project-local: stored at <dir>/http-cache.db, where <dir> is the project's
// .srt-data. Entries live forever; delete the file to drop them.
//
// Cached: GET (and HEAD) 2xx responses with no Authorization on the request
// and no Cache-Control: no-store on either side. The cache key is
// method + "\n" + url, stored raw; headers are intentionally not part of the
// key. (The Bun predecessor hashed the key with sha256, which was cosmetic:
// old hashed entries simply miss and re-populate.)

import { Database } from "flux:sqlite"
import { dir } from "flux:fs"
import { join } from "flux:path"

const CACHE_FILE = "http-cache.db"

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

export async function initCache(opts: { dir: string }) {
  await dir(opts.dir).create()
  let d = await Database.open(join(opts.dir, CACHE_FILE), "rw+")
  await d.exec(`CREATE TABLE IF NOT EXISTS entries (
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
  return method + "\n" + url
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

export async function get(method: string, url: string): Promise<Entry | null> {
  if (!db || !enabled) return null
  let row = await db
    .query("SELECT method, url, status, headers, body, cached_at FROM entries WHERE key = ?")
    .get([keyFor(method, url)])
  if (!row) return null
  return {
    method: row.method as string,
    url: row.url as string,
    status: row.status as number,
    headers: JSON.parse(row.headers as string),
    body: row.body as Uint8Array,
    cachedAt: row.cached_at as number,
  }
}

export async function put(
  method: string,
  url: string,
  status: number,
  headers: Record<string, string>,
  body: Uint8Array,
) {
  if (!db || !enabled) return
  if (status < 200 || status >= 300) return
  if (hasNoStore(headers["cache-control"] ?? null)) return
  await db.run(
    `INSERT OR REPLACE INTO entries
     (key, method, url, status, headers, body, cached_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
    [keyFor(method, url), method, url, status, JSON.stringify(headers), body, Date.now()],
  )
}
