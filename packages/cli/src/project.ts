import { existsSync, readFileSync } from "node:fs"
import { basename, dirname, resolve } from "node:path"

// Project configuration lives in the `solidrt` key of the nearest package.json
// above the entry file (okf/plans/client-storage-updates.md):
//
//   "solidrt": {
//     "appId": "com.example.app",   // stable identity: storage dir, Android package id
//     "org": "Example",             // publisher, pref-path org component
//     "displayName": "Example App", // pref-path app component
//     "fonts": { ... }              // see fonts.ts
//   }
//
// Everything defaults from the package name (or the entry filename when there
// is no package.json) so a dev project needs zero config; `srt pack` warns
// when appId is defaulted, since a distributed app should pin its identity.

export function findProjectPackage(sourcePath: string): { dir: string; pkg: any } | null {
  let dir = resolve(dirname(sourcePath))
  while (true) {
    let pkgPath = resolve(dir, "package.json")
    if (existsSync(pkgPath)) {
      return { dir, pkg: JSON.parse(readFileSync(pkgPath, "utf8")) }
    }
    let parent = dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

export type AppIdentity = { appId: string; org: string; displayName: string; defaulted: boolean }

function fail(message: string): never {
  console.error(message)
  process.exit(1)
}

// Storage directory component (matches the runtime's safe_component check).
let APP_ID_PATTERN = /^[A-Za-z0-9._-]+$/

// Derived values are sanitized into a valid appId; explicit config must
// already be valid (throw-in-dev policy: a bad value fails the command).
function sanitizeAppId(name: string): string {
  let id = name.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^[-.]+|[-.]+$/g, "")
  return id === "" || id === "." || id === ".." ? "app" : id
}

function checkField(value: string, what: string) {
  if (Buffer.byteLength(value, "utf8") > 255) fail(`"solidrt": ${what} is too long (max 255 bytes)`)
  if (/[/\\]/.test(value)) fail(`"solidrt": ${what} must not contain path separators`)
  if (value === "") fail(`"solidrt": ${what} must not be empty`)
}

// Resolve the app identity for a pack. All three fields are guaranteed
// non-empty and 255 bytes max (the trailer encoding's length prefix).
export function loadAppIdentity(sourcePath: string): AppIdentity {
  let project = findProjectPackage(sourcePath)
  let config = project?.pkg.solidrt ?? {}
  let fallbackName = project?.pkg.name ?? basename(sourcePath).replace(/\.[jt]sx?$/, "")

  for (let key of ["appId", "org", "displayName"]) {
    if (key in config && typeof config[key] !== "string") fail(`"solidrt": "${key}" must be a string`)
  }

  let appId: string
  let defaulted = typeof config.appId !== "string"
  if (defaulted) {
    appId = sanitizeAppId(fallbackName)
  } else {
    appId = config.appId
    if (!APP_ID_PATTERN.test(appId) || appId === "." || appId === "..") {
      fail(`"solidrt": "appId" must match ${APP_ID_PATTERN} (reverse-DNS recommended, e.g. "com.example.app")`)
    }
  }
  let displayName = config.displayName ?? (project?.pkg.name || fallbackName)
  let org = config.org ?? displayName
  checkField(appId, '"appId"')
  checkField(displayName, '"displayName"')
  checkField(org, '"org"')
  return { appId, org, displayName, defaulted }
}
